//! Unit tests for the scene subsystem, grouped into concern-named submodules.
//! Kept inline (private-item access) — shared fixtures live at this module's
//! root; each nested `mod` covers one concern and pulls them in via `super::*`.
//! The pre-game (login/creation/select/MOTD) suites moved to `grim-auth`; what
//! remains exercises copyover resume, output formatting, and in-game dispatch.

use bevy::prelude::*;
use chrono::Utc;
use grim_actor::{InRoom, Linkdead, OutputHistory, Player, Role, StoredCharacter};
use grim_channel::ChannelPlugin;
use grim_core::components::Name as GrimName;
use grim_core::components::*;
use grim_core::events::*;
// Explicit named import shadows the glob'd `bevy::prelude::Command` trait.
use grim_core::events::Command;
use grim_core::GrimId;
use grim_networking::{Connection, ConnectionEstablished, ConnectionInput, ConnectionOutput};
use grim_persistence::PersistencePlugin;
use grim_world::{Room, StartingRoom, WorldPlugin};
use std::net::SocketAddr;

use crate::ScenePlugin;

// ─── Shared fixtures ─────────────────────

fn test_app() -> App {
    // Clean up persisted data to avoid cross-test contamination
    let _ = std::fs::remove_dir_all("data/accounts");
    let _ = std::fs::remove_dir_all("data/characters");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(WorldPlugin);
    app.add_plugins(ChannelPlugin);
    app.add_plugins(PersistencePlugin);
    app.add_plugins(ScenePlugin);
    // Telnet protocol messages not registered by the above plugins
    app.add_message::<ConnectionEstablished>()
        .add_message::<ConnectionInput>();
    app
}

fn spawn_room(app: &mut App) -> Entity {
    app.world_mut()
        .spawn((
            Room {
                id: GrimId::new(),
                friendly_id: "room1".into(),
                name: "Room".into(),
                description: "A room.".into(),
                area: Entity::PLACEHOLDER,
            },
            GrimName("Room".into()),
        ))
        .id()
}

fn spawn_ingame(app: &mut App, conn: Entity, stored: StoredCharacter) -> Entity {
    let (name, actor, character) = stored.into_components();
    let char_entity = app
        .world_mut()
        .spawn((
            name,
            actor,
            character,
            InRoom {
                room: Entity::PLACEHOLDER,
            },
            Player { connection: conn },
        ))
        .id();
    let mut client = Client::new(conn);
    client.state = ClientState::InGame;
    client.character = Some(char_entity);
    app.world_mut().spawn(client);
    char_entity
}

/// A flat [`StoredCharacter`] fixture ("Hero" by default). The post-split disk
/// surface; split into `Name + Actor + Character` when spawning a live entity.
fn make_character(roles: Vec<Role>) -> StoredCharacter {
    StoredCharacter {
        id: GrimId::new(),
        name: "Hero".into(),
        account_id: GrimId::new(),
        created_at: Utc::now(),
        last_room: None,
        roles,
        gender: Gender::Neutral,
        race: String::new(),
        class: String::new(),
        level: 1,
        title: None,
        restrings: std::collections::HashMap::new(),
    }
}

// ─── Copyover resume ─────────────────────

mod reconnect {
    use super::*;
    use grim_networking::ConnectionResumed;

    /// Copyover resume of a linkdead resident: the resident has `Character +
    /// Linkdead` and no `Player`; resuming it must attach a `Player` AND clear
    /// `Linkdead` in the same step, so it ends online and not linkdead.
    #[test]
    fn resume_linkdead_resident_clears_linkdead_and_attaches_player() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let account = Account {
            id: GrimId::new(),
            identifier: "resume@example.com".into(),
            password_hash: String::new(),
            characters: vec![],
            created_at: Utc::now(),
        };
        let account_id = account.id;
        app.world_mut().spawn(account);

        let mut stored = make_character(Vec::new());
        stored.name = "Test".into();
        stored.account_id = account_id;
        let (name, actor, character) = stored.into_components();
        let char_entity = app
            .world_mut()
            .spawn((name, actor, character, InRoom { room }, Linkdead))
            .id();

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 7,
                addr: "127.0.0.1:12377".parse::<SocketAddr>().unwrap(),
                echo_hidden: false,
            })
            .id();
        app.world_mut().write_message(ConnectionResumed {
            connection: conn,
            character: "Test".into(),
        });
        app.update();

        let player = app
            .world()
            .get::<Player>(char_entity)
            .expect("resume must attach a Player (online)");
        assert_eq!(
            player.connection, conn,
            "resume must attach the resumed connection, not some other"
        );
        assert!(
            app.world().get::<Linkdead>(char_entity).is_none(),
            "resume must clear Linkdead — not both online and linkdead"
        );
    }
}

// ─── Output formatting & broadcast ───────

mod output_format {
    use super::*;

    /// Verify that format_output broadcasts SayEvent to room occupants.
    #[test]
    fn format_output_say_broadcast() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let actor_conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        let observer_conn = app
            .world_mut()
            .spawn(Connection {
                id: 2,
                addr: "127.0.0.1:12346".parse().unwrap(),
                echo_hidden: false,
            })
            .id();

        let actor = app
            .world_mut()
            .spawn((
                GrimName("Hero".into()),
                InRoom { room },
                Player {
                    connection: actor_conn,
                },
                OutputHistory::with_max(100),
            ))
            .id();
        let _observer = app
            .world_mut()
            .spawn((
                GrimName("Bystander".into()),
                InRoom { room },
                Player {
                    connection: observer_conn,
                },
                OutputHistory::with_max(100),
            ))
            .id();

        app.world_mut().write_message(SayEvent {
            room,
            actor,
            text: "hello".into(),
        });
        app.world_mut().write_message(InfoMessage {
            target: actor,
            text: "You say, 'hello'\n".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();

        assert!(
            outputs
                .iter()
                .any(|o| o.connection == observer_conn && o.text.contains("Hero says")),
            "observer should get broadcast"
        );
        assert!(
            outputs
                .iter()
                .any(|o| o.connection == actor_conn && o.text.contains("You say")),
            "actor should get echo"
        );
    }

    /// `gecho` reaches everyone including the sender; another admin sees it
    /// attributed (`Name> text`) while the sender and non-admins see raw text.
    #[test]
    fn format_output_gecho_attributes_for_other_admins_only() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let mk_conn = |app: &mut App, id: usize, port: u16| -> Entity {
            app.world_mut()
                .spawn(Connection {
                    id,
                    addr: format!("127.0.0.1:{port}").parse().unwrap(),
                    echo_hidden: false,
                })
                .id()
        };
        let sender_conn = mk_conn(&mut app, 1, 12345);
        let admin2_conn = mk_conn(&mut app, 2, 12346);
        let normal_conn = mk_conn(&mut app, 3, 12347);

        let admin_char = |name: &str, conn: Entity| {
            let (gname, actor, character) = StoredCharacter {
                id: GrimId::new(),
                account_id: GrimId::new(),
                name: name.into(),
                created_at: Utc::now(),
                last_room: None,
                roles: vec![Role::Admin],
                gender: Gender::Neutral,
                race: String::new(),
                class: String::new(),
                level: 1,
                title: None,
                restrings: std::collections::HashMap::new(),
            }
            .into_components();
            (
                gname,
                actor,
                character,
                InRoom { room },
                Player { connection: conn },
                OutputHistory::with_max(100),
            )
        };

        let sender = app.world_mut().spawn(admin_char("Boss", sender_conn)).id();
        let _admin2 = app.world_mut().spawn(admin_char("Deputy", admin2_conn));
        let (gname, actor, character) = StoredCharacter {
            id: GrimId::new(),
            account_id: GrimId::new(),
            name: "Peon".into(),
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
            gender: Gender::Neutral,
            race: String::new(),
            class: String::new(),
            level: 1,
            title: None,
            restrings: std::collections::HashMap::new(),
        }
        .into_components();
        let _normal = app.world_mut().spawn((
            gname,
            actor,
            character,
            InRoom { room },
            Player {
                connection: normal_conn,
            },
            OutputHistory::with_max(100),
        ));

        app.world_mut().write_message(GlobalEcho {
            actor: sender,
            text: "reboot".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();
        let text_for = |conn: Entity| -> String {
            outputs
                .iter()
                .filter(|o| o.connection == conn)
                .map(|o| o.text.clone())
                .collect()
        };

        // Sender sees raw text, not attributed to themselves.
        let sender_text = text_for(sender_conn);
        assert!(sender_text.contains("reboot"), "sender should see the echo");
        assert!(
            !sender_text.contains("Boss>"),
            "sender should not see own name prefix"
        );
        // Another admin sees it attributed.
        assert!(
            text_for(admin2_conn).contains("Boss> reboot"),
            "other admin should see attributed echo"
        );
        // Non-admin sees raw text only.
        let normal_text = text_for(normal_conn);
        assert!(normal_text.contains("reboot"), "non-admin should see echo");
        assert!(
            !normal_text.contains("Boss>"),
            "non-admin should not see attribution"
        );
    }

    /// Verify that format_output handles LoginAnnounce (broadcast_global path).
    #[test]
    fn format_output_login_announce() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        app.world_mut().spawn((
            GrimName("Hero".into()),
            InRoom { room },
            Player { connection: conn },
            OutputHistory::with_max(100),
        ));

        app.world_mut().write_message(LoginAnnounce {
            name: "Hero".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();
        assert!(
            outputs
                .iter()
                .any(|o| o.text.contains("Hero has connected")),
            "should announce login"
        );
    }

    /// A `ServerBroadcast` reaches every connected player (shutdown warnings).
    #[test]
    fn server_broadcast_reaches_connected_players() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        app.world_mut().spawn((
            GrimName("Hero".into()),
            InRoom { room },
            Player { connection: conn },
            OutputHistory::with_max(100),
        ));

        app.world_mut().write_message(ServerBroadcast {
            text: "{R[SERVER]{x Restarting in {Y15{x seconds.\n".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();
        assert!(
            outputs
                .iter()
                .any(|o| o.connection == conn && o.text.contains("Restarting in")),
            "connected player should receive the broadcast"
        );
    }

    /// Verify that format_output handles LogoutAnnounce.
    #[test]
    fn format_output_logout_announce() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        app.world_mut().spawn((
            GrimName("Hero".into()),
            InRoom { room },
            Player { connection: conn },
            OutputHistory::with_max(100),
        ));

        app.world_mut().write_message(LogoutAnnounce {
            name: "Hero".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();
        assert!(
            outputs
                .iter()
                .any(|o| o.text.contains("Hero has disconnected")),
            "should announce logout"
        );
    }

    /// Verify that format_output handles LinkdeadAnnounce (reconnecting).
    #[test]
    fn format_output_linkdead_announce() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        app.world_mut().spawn((
            GrimName("Hero".into()),
            InRoom { room },
            Player { connection: conn },
            OutputHistory::with_max(100),
        ));

        app.world_mut().write_message(LinkdeadAnnounce {
            name: "Hero".into(),
            reconnecting: true,
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();
        assert!(
            outputs
                .iter()
                .any(|o| o.text.contains("Hero has reconnected")),
            "should announce reconnect"
        );
    }

    // ── format_output: look_room with missing room ──
    #[test]
    fn format_output_look_room_room_not_found() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        // Write LookRoom for a non-existent room → should not panic
        app.world_mut().write_message(LookRoom {
            target: Entity::PLACEHOLDER,
            room: Entity::PLACEHOLDER,
        });
        app.update();

        // No crash = success
    }

    // ── format_output: admins see room ids in the title, players don't ──
    #[test]
    fn format_output_admin_sees_room_ids() {
        for (admin, expect_ids) in [(true, true), (false, false)] {
            let mut app = test_app();
            let room = spawn_room(&mut app);
            app.world_mut().insert_resource(StartingRoom(room));
            let conn = app
                .world_mut()
                .spawn(Connection {
                    id: 1,
                    addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
                    echo_hidden: false,
                })
                .id();
            let roles = if admin { vec![Role::Admin] } else { vec![] };
            let target = spawn_ingame(&mut app, conn, make_character(roles));
            app.world_mut().write_message(LookRoom { target, room });
            app.update();

            let msgs = app.world().resource::<Messages<ConnectionOutput>>();
            let mut cursor = msgs.get_cursor();
            let text: String = cursor
                .read(msgs)
                .filter(|o| o.connection == conn)
                .map(|o| o.text.clone())
                .collect();

            if expect_ids {
                assert!(
                    text.contains("entity:")
                        && text.contains("grim:")
                        && text.contains("slug:room1"),
                    "admin should see room ids; got:\n{text}"
                );
            } else {
                assert!(
                    !text.contains("entity:") && !text.contains("grim:") && !text.contains("slug:"),
                    "normal player must not see room ids; got:\n{text}"
                );
            }
        }
    }

    // ── format_output: look_entity with missing subject name ──
    #[test]
    fn format_output_look_entity_not_found() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        // Entity with no GrimName component → lookup fails, format_output continues
        let nameless = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(LookEntity {
            target: Entity::PLACEHOLDER,
            subject: nameless,
        });
        app.update();

        // No crash = success
    }

    // ── format_output: move broadcasts to from/to rooms ──
    #[test]
    fn format_output_move_broadcasts() {
        let mut app = test_app();
        let from_room = spawn_room(&mut app);
        let to_room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(from_room));

        let actor_conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        let observer_conn = app
            .world_mut()
            .spawn(Connection {
                id: 2,
                addr: "127.0.0.1:12346".parse().unwrap(),
                echo_hidden: false,
            })
            .id();

        let actor = app
            .world_mut()
            .spawn((
                GrimName("Mover".into()),
                InRoom { room: from_room },
                Player {
                    connection: actor_conn,
                },
                OutputHistory::with_max(100),
            ))
            .id();
        let _observer = app
            .world_mut()
            .spawn((
                GrimName("Watcher".into()),
                InRoom { room: from_room },
                Player {
                    connection: observer_conn,
                },
                OutputHistory::with_max(100),
            ))
            .id();

        app.world_mut().write_message(MoveEvent {
            actor,
            from: from_room,
            to: to_room,
            direction: grim_core::cardinal::Cardinal::North,
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();
        // Observer in from_room should see departure
        assert!(
            outputs
                .iter()
                .any(|o| o.connection == observer_conn && o.text.contains("Mover leaves")),
            "Observer should see departure message"
        );
    }
}

// ─── In-game command dispatch ────────────

mod ingame_commands {
    use super::*;

    // ── handle_client_input: InGame with unknown command ──
    #[test]
    fn ingame_unknown_command_shows_error() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();

        let char_entity = app
            .world_mut()
            .spawn((
                GrimName("Hero".into()),
                InRoom { room },
                Player { connection: conn },
            ))
            .id();

        let mut client = Client::new(conn);
        client.state = ClientState::InGame;
        client.character = Some(char_entity);
        app.world_mut().spawn(client);

        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "blargh".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();
        assert!(
            outputs
                .iter()
                .any(|o| o.connection == conn && o.text.contains("Unknown command")),
            "Unknown command should show error message"
        );
    }

    /// A non-admin `shutdown` is indistinguishable from an unknown command:
    /// same text, and the same framing (direct output, no prepended newline).
    #[test]
    fn ingame_shutdown_masked_for_non_admin() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));
        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        spawn_ingame(&mut app, conn, make_character(Vec::new()));

        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "shutdown 30".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let out = cursor
            .read(msgs)
            .find(|o| o.connection == conn)
            .expect("expected a response");
        assert_eq!(out.text, "Unknown command. Type 'commands' for a list.\n");
        assert!(!out.prepend_newline, "must match unknown-command framing");

        // And the command was not forwarded to the engine.
        let engine = app.world().resource::<Messages<EngineCommand>>();
        assert_eq!(engine.get_cursor().read(engine).count(), 0);
    }

    /// An admin `shutdown` is accepted (queued), never masked.
    #[test]
    fn ingame_shutdown_allowed_for_admin() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));
        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        spawn_ingame(&mut app, conn, make_character(vec![Role::Admin]));

        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "shutdown 30".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        assert!(
            !cursor
                .read(msgs)
                .any(|o| o.connection == conn && o.text.contains("Unknown command")),
            "admin shutdown must not be masked"
        );

        // Positively confirm it was accepted (a silent drop would still pass the
        // not-masked check above). Depending on the command cooldown, after one
        // update it is either still queued or already dispatched as an
        // EngineCommand — accept either so the test doesn't depend on timing.
        let engine = app.world().resource::<Messages<EngineCommand>>();
        let dispatched = engine
            .get_cursor()
            .read(engine)
            .any(|e| matches!(e.command, Command::Shutdown { seconds: 30 }));
        let mut clients = app.world_mut().query::<&Client>();
        let queued = clients
            .iter(app.world())
            .find(|c| c.connection == conn)
            .is_some_and(|c| {
                matches!(
                    c.input_queue.front(),
                    Some(Command::Shutdown { seconds: 30 })
                )
            });
        assert!(
            queued || dispatched,
            "admin shutdown should be queued or dispatched, not dropped"
        );
    }

    /// A non-admin `gecho` is masked exactly like an unknown command and never
    /// forwarded to the engine.
    #[test]
    fn ingame_gecho_masked_for_non_admin() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));
        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        spawn_ingame(&mut app, conn, make_character(Vec::new()));

        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "gecho hello world".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let out = cursor
            .read(msgs)
            .find(|o| o.connection == conn)
            .expect("expected a response");
        assert_eq!(out.text, "Unknown command. Type 'commands' for a list.\n");
        assert!(!out.prepend_newline, "must match unknown-command framing");

        let engine = app.world().resource::<Messages<EngineCommand>>();
        assert_eq!(engine.get_cursor().read(engine).count(), 0);
    }

    // ── handle_client_input: InGame with blank line ──
    #[test]
    fn ingame_blank_line_triggers_prompt() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();

        let char_entity = app
            .world_mut()
            .spawn((
                GrimName("Hero".into()),
                InRoom { room },
                Player { connection: conn },
            ))
            .id();

        let mut client = Client::new(conn);
        client.state = ClientState::InGame;
        client.character = Some(char_entity);
        app.world_mut().spawn(client);

        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();
        assert!(
            outputs
                .iter()
                .any(|o| o.connection == conn && o.text == " "),
            "Blank line should write a space to trigger prompt"
        );
    }
}
