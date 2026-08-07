//! Unit tests for the pre-game (auth) flow, grouped into concern-named
//! submodules. Kept inline (private-item access) — shared fixtures live at this
//! module's root; each nested `mod` covers one concern and pulls them in via
//! `super::*`. The harness composes ScenePlugin (the session core this layers
//! on) + AuthPlugin, so the full login → in-game loop is exercised.

use bevy::prelude::*;
use chrono::Utc;
use grim_actor::{Actor, Character, InRoom, Linkdead, OutputHistory, Player, StoredCharacter};
use grim_channel::ChannelPlugin;
use grim_core::components::Name as GrimName;
use grim_core::components::*;
use grim_core::events::*;
use grim_core::GrimId;
use grim_networking::{
    Connection, ConnectionEstablished, ConnectionInput, ConnectionOutput, DisconnectRequest,
};
use grim_persistence::{PersistenceConfig, PersistencePlugin};
use grim_world::{ClassRegistry, RaceRegistry, Room, StartingRoom, WorldPlugin};
use std::net::SocketAddr;

use grim_scene::{JustEnteredWorld, ScenePlugin};

use crate::greeter::handle_connection_established;
use crate::input::handle_pregame_input;
use crate::validation::{hash_password, ReservedNamePrefixes};
use crate::AuthPlugin;

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
    app.add_plugins(AuthPlugin);
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

/// Fresh app rooted at a unique temp data dir so on-disk fixtures are
/// isolated (no `data/` contention with the other tests).
fn test_app_in(dir: &std::path::Path) -> App {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir.join("characters")).unwrap();
    std::fs::create_dir_all(dir.join("accounts")).unwrap();
    let mut app = App::new();
    app.insert_resource(PersistenceConfig {
        dir: dir.to_path_buf(),
    });
    app.add_plugins(MinimalPlugins);
    app.add_plugins(WorldPlugin);
    app.add_plugins(ChannelPlugin);
    app.add_plugins(PersistencePlugin);
    app.add_plugins(ScenePlugin);
    app.add_plugins(AuthPlugin);
    app.add_message::<ConnectionEstablished>()
        .add_message::<ConnectionInput>();
    app
}

fn unique_dir(tag: &str) -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("grim-scene-{tag}-{}-{}", std::process::id(), n))
}

/// Write a character to the configured `characters/` dir as if it were saved
/// there while logged out (no in-world entity).
fn write_disk_char(dir: &std::path::Path, ch: &StoredCharacter) {
    std::fs::write(
        dir.join("characters").join(format!("{}.json", ch.name)),
        serde_json::to_string(ch).unwrap(),
    )
    .unwrap();
}

fn spawn_conn(app: &mut App, id: usize) -> Entity {
    app.world_mut()
        .spawn(Connection {
            id,
            addr: format!("127.0.0.1:{}", 10000 + id)
                .parse::<SocketAddr>()
                .unwrap(),
            echo_hidden: false,
        })
        .id()
}

// ─── Reconnect / takeover / duplicate-entity resolution ──────

mod reconnect {
    use super::*;

    /// Simulate name-based reconnect: type character name at login prompt,
    /// then password. The character has Linkdead — should reconnect.
    #[allow(clippy::too_many_lines)] // reason: end-to-end reconnect scenario; one linear flow reads clearer unsplit
    #[test]
    fn reconnect_by_name_on_linkdead_character() {
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

        app.world_mut().spawn(Client::new(conn));

        let account = Account {
            id: GrimId::new(),
            identifier: "test@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![],
            created_at: Utc::now(),
        };
        let account_id = account.id;
        let _account_entity = app.world_mut().spawn(account).id();

        let char_uuid = GrimId::new();
        let stored = StoredCharacter {
            id: char_uuid,
            name: "Test".into(),
            account_id,
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
            gender: Gender::Neutral,
            race: "human".into(),
            class: "warrior".into(),
            level: 1,
            title: None,
            restrings: std::collections::HashMap::new(),
        };
        let (name, actor, character) = stored.into_components();
        let char_entity = app
            .world_mut()
            .spawn((
                name,
                actor,
                character,
                Description("A test character.".into()),
                InRoom { room },
                Linkdead,
                OutputHistory::with_max(100),
            ))
            .id();
        // Buffer some output from "before the disconnect" to prove reconnect
        // does NOT replay it.
        app.world_mut()
            .get_mut::<OutputHistory>(char_entity)
            .unwrap()
            .push("STALE_BUFFERED_LINE\n");

        // Step 1: Send character name at login prompt
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "Test".into(),
        });
        app.update();

        // Check: client should now be in PasswordPrompt state
        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(app.world()).find(|(_, c)| c.connection == conn);
        assert!(found.is_some(), "Client should exist");
        let (_client_entity, client) = found.unwrap();
        assert_eq!(
            client.state,
            ClientState::PasswordPrompt {
                identifier: "test@example.com".into(),
                is_new: false,
                character: Some("Test".into()),
            },
            "Should be in PasswordPrompt state after name entry"
        );

        // Step 2: Send password
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "password".into(),
        });
        app.update();

        // Check: Linkdead should be removed, Player.connection should be Some
        let mut players = app.world_mut().query::<&Player>();
        let player = players.get(app.world(), char_entity);
        assert!(player.is_ok(), "Character should have Player component");
        assert_eq!(
            player.unwrap().connection,
            conn,
            "Player should be connected after reconnect"
        );

        let mut linkdead = app.world_mut().query::<&Linkdead>();
        assert!(
            linkdead.get(app.world(), char_entity).is_err(),
            "Linkdead should be removed after reconnect"
        );

        // Check: LinkdeadAnnounce was written with reconnecting: true
        let msg_resource = app.world().resource::<Messages<LinkdeadAnnounce>>();
        let mut cursor = msg_resource.get_cursor();
        let announces: Vec<&LinkdeadAnnounce> = cursor.read(msg_resource).collect();
        let has_reconnect = announces.iter().any(|a| a.reconnecting && a.name == "Test");
        assert!(
            has_reconnect,
            "Should emit LinkdeadAnnounce with reconnecting=true"
        );

        // No playback: the reconnecting player gets the notice but NOT the
        // buffered pre-disconnect output, and no auto-look.
        let out = app.world().resource::<Messages<ConnectionOutput>>();
        let mut oc = out.get_cursor();
        let text: String = oc
            .read(out)
            .filter(|o| o.connection == conn)
            .map(|o| o.text.clone())
            .collect();
        assert!(
            text.contains("Reconnecting..."),
            "expected reconnect notice; got:\n{text}"
        );
        assert!(
            !text.contains("STALE_BUFFERED_LINE"),
            "must not replay buffered output; got:\n{text}"
        );
        let lr = app.world().resource::<Messages<LookRoom>>();
        let mut lc = lr.get_cursor();
        assert!(
            lc.read(lr).all(|e| e.target != char_entity),
            "reconnect must not auto-look the room"
        );
    }

    /// Simulate email-based reconnect: type email, then password, then select
    /// character from menu. Character has Linkdead — should reconnect.
    #[allow(clippy::too_many_lines)] // reason: end-to-end reconnect scenario; one linear flow reads clearer unsplit
    #[test]
    fn reconnect_by_email_on_linkdead_character() {
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

        app.world_mut().spawn(Client::new(conn));

        let char_uuid = GrimId::new();
        let account = Account {
            id: GrimId::new(),
            identifier: "test@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![char_uuid],
            created_at: Utc::now(),
        };
        let account_id = account.id;
        app.world_mut().spawn(account);

        let stored = StoredCharacter {
            id: char_uuid,
            name: "Test".into(),
            account_id,
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
            gender: Gender::Neutral,
            race: "human".into(),
            class: "warrior".into(),
            level: 1,
            title: None,
            restrings: std::collections::HashMap::new(),
        };
        let (name, actor, character) = stored.into_components();
        let char_entity = app
            .world_mut()
            .spawn((
                name,
                actor,
                character,
                Description("A test character.".into()),
                InRoom { room },
                Linkdead,
                OutputHistory::with_max(100),
            ))
            .id();

        // Step 1: Send email at login prompt
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "test@example.com".into(),
        });
        app.update();

        // Check: client should now be in PasswordPrompt state
        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(app.world()).find(|(_, c)| c.connection == conn);
        assert!(found.is_some(), "Client should exist");
        let (_client_entity, client) = found.unwrap();
        assert_eq!(
            client.state,
            ClientState::PasswordPrompt {
                identifier: "test@example.com".into(),
                is_new: false,
                character: None,
            },
            "Should be in PasswordPrompt state after email entry"
        );

        // Step 2: Send password
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "password".into(),
        });
        app.update();

        // Check: client should now be in CharacterSelect state
        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(app.world()).find(|(_, c)| c.connection == conn);
        assert!(found.is_some(), "Client should exist");
        let (_client_entity, client) = found.unwrap();
        assert_eq!(
            client.state,
            ClientState::CharacterSelect,
            "Should be in CharacterSelect after password for email login"
        );

        // Step 3: Select character by number
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "1".into(),
        });
        app.update();

        // Check: Linkdead should be removed, Player.connection should be Some
        let mut players = app.world_mut().query::<&Player>();
        let player = players.get(app.world(), char_entity);
        assert!(player.is_ok(), "Character should have Player component");
        assert_eq!(
            player.unwrap().connection,
            conn,
            "Player should be connected after reconnect"
        );

        let mut linkdead = app.world_mut().query::<&Linkdead>();
        assert!(
            linkdead.get(app.world(), char_entity).is_err(),
            "Linkdead should be removed after reconnect"
        );
        // Check: LinkdeadAnnounce was written with reconnecting: true
        let msg_resource = app.world().resource::<Messages<LinkdeadAnnounce>>();
        let mut cursor = msg_resource.get_cursor();
        let announces: Vec<&LinkdeadAnnounce> = cursor.read(msg_resource).collect();
        let has_reconnect = announces.iter().any(|a| a.reconnecting && a.name == "Test");
        assert!(
            has_reconnect,
            "Should emit LinkdeadAnnounce with reconnecting=true"
        );
    }

    /// Two character entities share a name (one linkdead, one stale without
    /// Linkdead). Login-by-name carries the NAME; entering the world must pick
    /// the linkdead entity to reconnect, never the stale duplicate.
    #[test]
    fn duplicate_entity_name_login_finds_wrong_one() {
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
        app.world_mut().spawn(Client::new(conn));

        let account_uuid = GrimId::new();
        let account = Account {
            id: account_uuid,
            identifier: "test@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![],
            created_at: Utc::now(),
        };
        app.world_mut().spawn(account);

        // Both copies share the character shape and differ only by id + build —
        // build them from one closure to keep the test flat.
        let make_char = |id: GrimId, race: &str, class: &str| StoredCharacter {
            id,
            name: "Test".into(),
            account_id: account_uuid,
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
            gender: Gender::Neutral,
            race: race.into(),
            class: class.into(),
            level: 1,
            title: None,
            restrings: std::collections::HashMap::new(),
        };

        // Entity A: stale — loaded from disk, no Linkdead
        let stale_uuid = GrimId::new();
        let (name, actor, character) = make_char(stale_uuid, "human", "warrior").into_components();
        app.world_mut()
            .spawn((name, actor, character, Description("Stale copy.".into())));

        // Entity B: real — in-world, went linkdead
        let (name, actor, character) = make_char(GrimId::new(), "", "").into_components();
        let real_entity = app
            .world_mut()
            .spawn((
                name,
                actor,
                character,
                Description("Real character.".into()),
                InRoom { room },
                Linkdead,
                OutputHistory::with_max(100),
            ))
            .id();

        // Send character name at login prompt
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "Test".into(),
        });
        app.update();

        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(app.world()).find(|(_, c)| c.connection == conn);
        assert!(found.is_some(), "Client should exist");
        let (_client_entity, client) = found.unwrap();

        match &client.state {
            ClientState::PasswordPrompt { character, .. } => {
                assert_eq!(
                    character.as_deref(),
                    Some("Test"),
                    "login-by-name should carry the character name"
                );
            }
            other => panic!("Expected PasswordPrompt, got {:?}", other),
        }

        // Send password → enter the world. The linkdead (real) entity must be
        // reconnected, and the stale duplicate left untouched.
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "password".into(),
        });
        app.update();

        let mut players = app.world_mut().query::<&Player>();
        assert!(
            players
                .get(app.world(), real_entity)
                .is_ok_and(|p| p.connection == conn),
            "the linkdead entity should be reconnected"
        );
        let mut linkdead = app.world_mut().query::<&Linkdead>();
        assert!(
            linkdead.get(app.world(), real_entity).is_err(),
            "Linkdead should be cleared on the reconnected entity"
        );
    }

    /// Duplicate entity via email login + character select menu.
    #[test]
    fn duplicate_entity_email_login_finds_wrong_one() {
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
        app.world_mut().spawn(Client::new(conn));

        let account_uuid = GrimId::new();
        let real_uuid = GrimId::new();
        let stale_uuid = GrimId::new();
        let account = Account {
            id: account_uuid,
            identifier: "test@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![real_uuid, stale_uuid],
            created_at: Utc::now(),
        };
        app.world_mut().spawn(account);

        // Both entities share the same character shape (name/account/build) and
        // differ only by id — build them from one closure to keep the test flat.
        let make_char = |id: GrimId| StoredCharacter {
            id,
            name: "Test".into(),
            account_id: account_uuid,
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
            gender: Gender::Neutral,
            race: "human".into(),
            class: "warrior".into(),
            level: 1,
            title: None,
            restrings: std::collections::HashMap::new(),
        };

        // Entity A: stale (no Linkdead)
        let (name, actor, character) = make_char(stale_uuid).into_components();
        app.world_mut()
            .spawn((name, actor, character, Description("Stale copy.".into())));

        // Entity B: real (with Linkdead)
        let (name, actor, character) = make_char(real_uuid).into_components();
        let real_entity = app
            .world_mut()
            .spawn((
                name,
                actor,
                character,
                Description("Real character.".into()),
                InRoom { room },
                Linkdead,
                OutputHistory::with_max(100),
            ))
            .id();

        // Step 1: Send email
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "test@example.com".into(),
        });
        app.update();

        // Step 2: Send password
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "password".into(),
        });
        app.update();

        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(app.world()).find(|(_, c)| c.connection == conn);
        assert!(found.is_some(), "Client should exist");
        let (_client_entity, client) = found.unwrap();
        assert_eq!(
            client.state,
            ClientState::CharacterSelect,
            "Should be in CharacterSelect after email login"
        );

        // Step 3: Select character "1"
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "1".into(),
        });
        app.update();

        let mut players = app.world_mut().query::<&Player>();
        let player = players.get(app.world(), real_entity);
        assert!(player.is_ok(), "Character should have Player component");
        assert_eq!(
            player.unwrap().connection,
            conn,
            "Player should be connected after reconnect"
        );

        let mut linkdead = app.world_mut().query::<&Linkdead>();
        assert!(
            linkdead.get(app.world(), real_entity).is_err(),
            "Linkdead should be removed after reconnect"
        );
    }

    /// Reconnecting a linkdead character by name reuses the existing entity —
    /// it is not duplicated by a stray disk load.
    #[test]
    fn linkdead_reconnect_reuses_entity_not_duplicated() {
        let dir = unique_dir("ldnodup");
        let mut app = test_app_in(&dir);
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let account_id = GrimId::new();
        let char_id = GrimId::new();
        app.world_mut().spawn(Account {
            id: account_id,
            identifier: "ld@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![char_id],
            created_at: Utc::now(),
        });
        // Also present on disk (as save-on-disconnect would leave it) to prove
        // reconnect reuses the resident entity rather than spawning a disk copy.
        let ch = StoredCharacter {
            id: char_id,
            name: "Linky".into(),
            account_id,
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
            gender: Gender::Neutral,
            race: "human".into(),
            class: "warrior".into(),
            level: 1,
            title: None,
            restrings: std::collections::HashMap::new(),
        };
        write_disk_char(&dir, &ch);
        let (name, actor, character) = ch.into_components();
        let char_entity = app
            .world_mut()
            .spawn((
                name,
                actor,
                character,
                Description("A linkdead character.".into()),
                InRoom { room },
                Linkdead,
                OutputHistory::with_max(100),
            ))
            .id();

        let conn = spawn_conn(&mut app, 1);
        app.world_mut().spawn(Client::new(conn));
        for line in ["Linky", "password"] {
            app.world_mut().write_message(ConnectionInput {
                connection: conn,
                text: line.into(),
            });
            app.update();
        }

        let mut q = app.world_mut().query::<(Entity, &GrimName, &Character)>();
        let entities: Vec<Entity> = q
            .iter(app.world())
            .filter(|(_, n, _)| n.0 == "Linky")
            .map(|(e, _, _)| e)
            .collect();
        assert_eq!(entities, vec![char_entity], "must reuse the same entity");

        let mut ld = app.world_mut().query::<&Linkdead>();
        assert!(ld.get(app.world(), char_entity).is_err());
        let mut pq = app.world_mut().query::<&Player>();
        assert_eq!(pq.get(app.world(), char_entity).unwrap().connection, conn);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ─── System ordering ─────────────────────

mod ordering {
    use super::*;

    /// Test that WITHOUT ordering, the first input is lost because the Client
    /// entity is spawned via deferred commands. This test manually adds the
    /// systems without ordering to demonstrate the problem.
    #[test]
    fn first_input_lost_without_ordering() {
        use grim_channel::ChannelPlugin;
        use grim_persistence::PersistencePlugin;
        use grim_world::WorldPlugin;
        use std::net::SocketAddr;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(WorldPlugin);
        app.add_plugins(ChannelPlugin);
        app.add_plugins(PersistencePlugin);
        app.add_message::<ConnectionOutput>()
            .add_message::<DisconnectRequest>()
            .add_message::<EngineCommand>()
            .add_message::<LookRoom>()
            .add_message::<LookEntity>()
            .add_message::<SayEvent>()
            .add_message::<YellEvent>()
            .add_message::<OocEvent>()
            .add_message::<MoveEvent>()
            .add_message::<InfoMessage>()
            .add_message::<LoginAnnounce>()
            .add_message::<LogoutAnnounce>()
            .add_message::<LinkdeadAnnounce>()
            .add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_systems(Update, handle_connection_established)
            .add_systems(Update, handle_pregame_input);

        let room = app
            .world_mut()
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
            .id();
        app.world_mut().insert_resource(StartingRoom(room));
        app.init_resource::<ReservedNamePrefixes>();
        app.init_resource::<JustEnteredWorld>();
        app.init_resource::<RaceRegistry>();
        app.init_resource::<ClassRegistry>();

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
                echo_hidden: false,
            })
            .id();

        app.world_mut().write_message(ConnectionEstablished {
            connection: conn,
            addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
        });
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "Test".into(),
        });

        app.update();

        let mut client_query = app.world_mut().query::<&Client>();
        let client_count = client_query.iter(app.world()).len();
        assert_eq!(client_count, 1, "Client should have been spawned");

        let mut client_state_query = app.world_mut().query::<&Client>();
        let client = client_state_query.iter(app.world()).next().unwrap();
        assert_eq!(
            client.state,
            ClientState::LoginPrompt,
            "First input should be lost: client should still be in LoginPrompt"
        );

        app.update();
        let mut client_state_query2 = app.world_mut().query::<&Client>();
        let client2 = client_state_query2.iter(app.world()).next().unwrap();
        assert_eq!(
            client2.state,
            ClientState::LoginPrompt,
            "Input was permanently lost: should still be in LoginPrompt"
        );
    }
}

// ─── Login / password / account-creation flow ────────

mod login_flow {
    use super::*;

    // ── LoginPrompt: empty password → wrong_password path ──
    #[test]
    fn login_prompt_empty_password_goes_wrong_password() {
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
        app.world_mut().spawn(Client::new(conn));

        let account = Account {
            id: GrimId::new(),
            identifier: "test@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![],
            created_at: Utc::now(),
        };
        app.world_mut().spawn(account);

        // Step 1: Type email at login prompt → PasswordPrompt
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "test@example.com".into(),
        });
        app.update();

        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(app.world()).find(|(_, c)| c.connection == conn);
        let (_client_entity, client) = found.unwrap();
        assert_eq!(
            client.state,
            ClientState::PasswordPrompt {
                identifier: "test@example.com".into(),
                is_new: false,
                character: None,
            }
        );

        // Step 2: Empty password → should fall back to LoginPrompt with wrong_password msg
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "".into(),
        });
        app.update();

        let mut query2 = app.world_mut().query::<(Entity, &Client)>();
        let found2 = query2.iter(app.world()).find(|(_, c)| c.connection == conn);
        let (_client_entity2, client2) = found2.unwrap();
        assert_eq!(
            client2.state,
            ClientState::LoginPrompt,
            "Empty password should revert to LoginPrompt"
        );

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();
        assert!(
            outputs
                .iter()
                .any(|o| o.connection == conn && o.text.contains("Invalid password")),
            "Should emit wrong_password output"
        );
    }

    // ── PasswordPrompt: wrong password (non-empty) ──
    #[test]
    fn password_prompt_wrong_password() {
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
        app.world_mut().spawn(Client::new(conn));

        let account = Account {
            id: GrimId::new(),
            identifier: "test@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![],
            created_at: Utc::now(),
        };
        app.world_mut().spawn(account);

        // Step 1: Type email → PasswordPrompt
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "test@example.com".into(),
        });
        app.update();

        // Step 2: Wrong password → stays in PasswordPrompt, shows error
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "wrongpassword".into(),
        });
        app.update();

        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(app.world()).find(|(_, c)| c.connection == conn);
        let (_client_entity, client) = found.unwrap();
        assert_eq!(
            client.state,
            ClientState::PasswordPrompt {
                identifier: "test@example.com".into(),
                is_new: false,
                character: None,
            },
            "Should remain in PasswordPrompt after wrong password"
        );

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();
        assert!(
            outputs
                .iter()
                .any(|o| o.connection == conn && o.text.contains("Invalid password")),
            "Should show invalid password message"
        );
    }

    // ── PasswordPrompt with is_new=true → creates account ──
    #[test]
    fn password_prompt_is_new_creates_account() {
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
        app.world_mut().spawn(Client::new(conn));

        // Step 1: Type new (unused) email → ConfirmCreate
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "newuser@example.com".into(),
        });
        app.update();

        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(app.world()).find(|(_, c)| c.connection == conn);
        let (_client_entity, client) = found.unwrap();
        assert_eq!(
            client.state,
            ClientState::ConfirmCreate {
                identifier: "newuser@example.com".into(),
            },
            "New email should go to ConfirmCreate"
        );

        // Step 2: Confirm → PasswordPrompt with is_new=true
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "yes".into(),
        });
        app.update();

        let mut query2 = app.world_mut().query::<(Entity, &Client)>();
        let found2 = query2.iter(app.world()).find(|(_, c)| c.connection == conn);
        let (_client_entity2, client2) = found2.unwrap();
        assert_eq!(
            client2.state,
            ClientState::PasswordPrompt {
                identifier: "newuser@example.com".into(),
                is_new: true,
                character: None,
            },
            "Confirmation should lead to PasswordPrompt with is_new=true"
        );

        // Step 3: Valid password → creates account, moves to CharacterSelect
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "securepass1".into(),
        });
        app.update();

        let mut query3 = app.world_mut().query::<(Entity, &Client)>();
        let found3 = query3.iter(app.world()).find(|(_, c)| c.connection == conn);
        let (_client_entity3, client3) = found3.unwrap();
        assert_eq!(
            client3.state,
            ClientState::CharacterSelect,
            "Successful account creation should lead to CharacterSelect"
        );
    }

    // ── ConfirmCreate with non-yes → LoginPrompt ──
    #[test]
    fn confirm_create_no_goes_login_prompt() {
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
        app.world_mut().spawn(Client::new(conn));

        // Step 1: Type new email → ConfirmCreate
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "newuser@example.com".into(),
        });
        app.update();

        // Step 2: "no" → back to LoginPrompt
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "no".into(),
        });
        app.update();

        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(app.world()).find(|(_, c)| c.connection == conn);
        let (_client_entity, client) = found.unwrap();
        assert_eq!(
            client.state,
            ClientState::LoginPrompt,
            "Refusing account creation should go back to LoginPrompt"
        );
    }
}

// ─── Character selection & creation ──────────

mod character_select {
    use super::*;

    // ── CharacterSelect: select third character ──
    #[test]
    fn character_select_third_character() {
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
        app.world_mut().spawn(Client::new(conn));

        let account_id = GrimId::new();
        let char_ids = vec![GrimId::new(), GrimId::new(), GrimId::new()];
        let account = Account {
            id: account_id,
            identifier: "test@example.com".into(),
            password_hash: hash_password("password"),
            characters: char_ids.clone(),
            created_at: Utc::now(),
        };
        let _account_entity = app.world_mut().spawn(account).id();

        // Spawn 3 characters (sorted C1, C2, C3 alphabetically by name)
        for (i, cid) in char_ids.iter().enumerate() {
            let (name, actor, character) = StoredCharacter {
                id: *cid,
                name: format!("C{}", i + 1),
                account_id,
                created_at: Utc::now(),
                last_room: None,
                roles: Vec::new(),
                gender: Gender::Neutral,
                race: "human".into(),
                class: "warrior".into(),
                level: 1,
                title: None,
                restrings: std::collections::HashMap::new(),
            }
            .into_components();
            app.world_mut().spawn((
                name,
                actor,
                character,
                Description(format!("Character {}.", i + 1)),
                InRoom { room },
            ));
        }

        // Login
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "test@example.com".into(),
        });
        app.update();

        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "password".into(),
        });
        app.update();

        // Verify in CharacterSelect
        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(app.world()).find(|(_, c)| c.connection == conn);
        let (_client_entity, client) = found.unwrap();
        assert_eq!(client.state, ClientState::CharacterSelect);

        // Select character 3
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "3".into(),
        });
        app.update();

        // Should transition to MotdPrompt (not linkdead)
        let mut query2 = app.world_mut().query::<(Entity, &Client)>();
        let found2 = query2.iter(app.world()).find(|(_, c)| c.connection == conn);
        let (_client_entity2, client2) = found2.unwrap();
        assert_eq!(
            client2.state,
            ClientState::MotdPrompt,
            "Selecting third character should work"
        );
    }

    // ── show_character_menu: linkdead characters show suffix ──
    #[test]
    fn character_menu_shows_linkdead_suffix() {
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
        app.world_mut().spawn(Client::new(conn));

        let account_id = GrimId::new();
        let char_uuid = GrimId::new();
        let account = Account {
            id: account_id,
            identifier: "test@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![char_uuid],
            created_at: Utc::now(),
        };
        app.world_mut().spawn(account);

        // Spawn linkdead character
        let (name, actor, character) = StoredCharacter {
            id: char_uuid,
            name: "Linky".into(),
            account_id,
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
        app.world_mut().spawn((
            name,
            actor,
            character,
            Description("A linkdead character.".into()),
            InRoom { room },
            Linkdead,
            OutputHistory::with_max(100),
        ));

        // Login → CharacterSelect should show linkdead suffix
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "test@example.com".into(),
        });
        app.update();

        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "password".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();
        assert!(
            outputs
                .iter()
                .any(|o| o.connection == conn && o.text.contains("(linkdead)")),
            "Character menu should show (linkdead) suffix for linkdead characters"
        );
    }

    // ── account isolation: a freshly-created account sees no other account's characters ──
    //
    // Regression. Creating an account spawns it with `commands.spawn`, whose
    // entity is not flushed until the next sync point — but `show_character_menu`
    // runs in the same system tick. The menu used to put its ownership check
    // inside `if let Ok(account) = accounts.get(..)`, so the unresolvable
    // just-spawned entity skipped the filter and listed EVERY character in the
    // world. A brand-new account B saw account A's characters.
    #[test]
    fn new_account_does_not_see_another_accounts_characters() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        // Account A already exists with a character.
        let account_a_id = GrimId::new();
        let char_a = GrimId::new();
        app.world_mut().spawn(Account {
            id: account_a_id,
            identifier: "a@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![char_a],
            created_at: Utc::now(),
        });
        let (name, actor, character) = StoredCharacter {
            id: char_a,
            name: "Aragorn".into(),
            account_id: account_a_id,
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
        app.world_mut().spawn((
            name,
            actor,
            character,
            Description("Heir of Isildur.".into()),
            InRoom { room },
            OutputHistory::with_max(100),
        ));

        // A new connection registers account B via a never-seen email.
        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 2,
                addr: "127.0.0.1:22222".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        app.world_mut().spawn(Client::new(conn));

        for line in ["b@example.com", "y", "password"] {
            app.world_mut().write_message(ConnectionInput {
                connection: conn,
                text: line.into(),
            });
            app.update();
        }

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let text: String = cursor
            .read(msgs)
            .filter(|o| o.connection == conn)
            .map(|o| o.text.clone())
            .collect();

        assert!(
            !text.contains("Aragorn"),
            "new account B must not see account A's character; got:\n{text}"
        );
        assert!(
            text.contains("no characters"),
            "new account B should be told it has no characters; got:\n{text}"
        );
    }

    #[test]
    fn create_character_reserved_name_is_configurable() {
        let dir = unique_dir("reserved");
        let mut app = test_app_in(&dir);
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let account_entity = app
            .world_mut()
            .spawn(Account {
                id: GrimId::new(),
                identifier: "r@example.com".into(),
                password_hash: hash_password("password"),
                characters: vec![],
                created_at: Utc::now(),
            })
            .id();
        let conn = spawn_conn(&mut app, 1);
        let mut client = Client::new(conn);
        client.state = ClientState::CreateCharacter;
        client.account = Some(account_entity);
        app.world_mut().spawn(client);

        // Default reserved list rejects a name starting with "self".
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "self".into(),
        });
        app.update();
        {
            let mut q = app.world_mut().query::<&GrimName>();
            assert!(
                q.iter(app.world()).all(|n| n.0 != "Self"),
                "a reserved name must not create a character"
            );
        }

        // Replace the reserved list with an empty one → the name is now allowed
        // and creation proceeds into the gender → race → class picker.
        app.world_mut()
            .insert_resource(ReservedNamePrefixes(vec![]));
        for line in ["self", "1", "human", "warrior"] {
            app.world_mut().write_message(ConnectionInput {
                connection: conn,
                text: line.into(),
            });
            app.update();
        }
        {
            let mut q = app.world_mut().query::<&GrimName>();
            assert!(
                q.iter(app.world()).any(|n| n.0 == "Self"),
                "an empty reserved list should allow the name"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ─── Connection lifecycle ────────────────

mod connection {
    use super::*;

    // ── handle_connection_established: banner rendering ──
    #[test]
    fn connection_established_shows_banner() {
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

        app.world_mut().write_message(ConnectionEstablished {
            connection: conn,
            addr: "127.0.0.1:12345".parse().unwrap(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ConnectionOutput> = cursor.read(msgs).collect();
        // Check that the banner (ASCII art) is in the output along with the login prompt
        assert!(
            outputs
                .iter()
                .any(|o| o.connection == conn && o.text.contains("______")),
            "Banner should contain ASCII art"
        );
        assert!(
            outputs
                .iter()
                .any(|o| o.connection == conn
                    && o.text.contains("character name or email address")),
            "Banner output should contain login prompt"
        );
    }
}

// ─── Disk-only character lifecycle ───────────

mod disk_lifecycle {
    use super::*;

    /// Logging in by the name of a character that only exists on disk spawns a
    /// fresh in-world entity and enters the world (no entity existed before).
    #[test]
    fn login_by_name_of_disk_only_character_spawns_and_enters() {
        let dir = unique_dir("diskname");
        let mut app = test_app_in(&dir);
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let account_id = GrimId::new();
        let char_id = GrimId::new();
        app.world_mut().spawn(Account {
            id: account_id,
            identifier: "disk@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![char_id],
            created_at: Utc::now(),
        });
        write_disk_char(
            &dir,
            &StoredCharacter {
                id: char_id,
                name: "Disky".into(),
                account_id,
                created_at: Utc::now(),
                last_room: None,
                roles: Vec::new(),
                gender: Gender::Neutral,
                race: "human".into(),
                class: "warrior".into(),
                level: 1,
                title: None,
                restrings: std::collections::HashMap::new(),
            },
        );

        let conn = spawn_conn(&mut app, 1);
        app.world_mut().spawn(Client::new(conn));

        // No in-world entity for Disky yet.
        {
            let mut q = app.world_mut().query::<(&Character, &GrimName)>();
            assert!(q.iter(app.world()).all(|(_, n)| n.0 != "Disky"));
        }

        // Type the name in the "wrong" case — login-by-name must be
        // case-insensitive even for a disk-only character.
        for line in ["disky", "password"] {
            app.world_mut().write_message(ConnectionInput {
                connection: conn,
                text: line.into(),
            });
            app.update();
        }

        // An entity was spawned and the client is at the MOTD, in the world.
        let mut q = app.world_mut().query::<(&Character, &GrimName, &Player)>();
        let matches: Vec<_> = q
            .iter(app.world())
            .filter(|(_, n, _)| n.0 == "Disky")
            .collect();
        assert_eq!(matches.len(), 1, "exactly one Disky entity should exist");
        assert_eq!(matches[0].2.connection, conn, "should be connected");

        let mut cq = app.world_mut().query::<&Client>();
        let client = cq.iter(app.world()).find(|c| c.connection == conn).unwrap();
        assert_eq!(client.state, ClientState::MotdPrompt);
        assert!(client.character.is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The character menu lists a disk-only character (plain, no online/linkdead
    /// suffix), and selecting it enters the world.
    #[test]
    fn menu_lists_and_selects_disk_only_character() {
        let dir = unique_dir("diskmenu");
        let mut app = test_app_in(&dir);
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let account_id = GrimId::new();
        let char_id = GrimId::new();
        app.world_mut().spawn(Account {
            id: account_id,
            identifier: "menu@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![char_id],
            created_at: Utc::now(),
        });
        write_disk_char(
            &dir,
            &StoredCharacter {
                id: char_id,
                name: "Diskette".into(),
                account_id,
                created_at: Utc::now(),
                last_room: None,
                roles: Vec::new(),
                gender: Gender::Neutral,
                race: "human".into(),
                class: "warrior".into(),
                level: 1,
                title: None,
                restrings: std::collections::HashMap::new(),
            },
        );

        let conn = spawn_conn(&mut app, 1);
        app.world_mut().spawn(Client::new(conn));

        for line in ["menu@example.com", "password"] {
            app.world_mut().write_message(ConnectionInput {
                connection: conn,
                text: line.into(),
            });
            app.update();
        }

        // Menu shows the disk-only character, with no residency suffix.
        {
            let msgs = app.world().resource::<Messages<ConnectionOutput>>();
            let mut cursor = msgs.get_cursor();
            let text: String = cursor
                .read(msgs)
                .filter(|o| o.connection == conn)
                .map(|o| o.text.clone())
                .collect();
            assert!(
                text.contains("Diskette"),
                "menu should list it; got:\n{text}"
            );
            assert!(!text.contains("(online)") && !text.contains("(linkdead)"));
        }

        // Select it → spawns and enters.
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "1".into(),
        });
        app.update();

        let mut q = app.world_mut().query::<(&Character, &GrimName)>();
        assert_eq!(
            q.iter(app.world())
                .filter(|(_, n)| n.0 == "Diskette")
                .count(),
            1,
            "selecting a disk-only character should spawn exactly one entity"
        );
        let mut cq = app.world_mut().query::<&Client>();
        let client = cq.iter(app.world()).find(|c| c.connection == conn).unwrap();
        assert_eq!(client.state, ClientState::MotdPrompt);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Logging into a character that is already online kicks the old session
    /// (message + disconnect) and hands the entity to the new connection.
    #[test]
    fn online_character_is_taken_over() {
        let dir = unique_dir("takeover");
        let mut app = test_app_in(&dir);
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let account_id = GrimId::new();
        let char_id = GrimId::new();
        app.world_mut().spawn(Account {
            id: account_id,
            identifier: "take@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![char_id],
            created_at: Utc::now(),
        });

        // The character is already online on `old_conn`.
        let old_conn = spawn_conn(&mut app, 1);
        let (name, actor, character) = StoredCharacter {
            id: char_id,
            name: "Twinsie".into(),
            account_id,
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
        let char_entity = app
            .world_mut()
            .spawn((
                name,
                actor,
                character,
                Description("Already online.".into()),
                InRoom { room },
                Player {
                    connection: old_conn,
                },
                OutputHistory::with_max(100),
            ))
            .id();

        // A second connection logs in as the same character by name.
        let new_conn = spawn_conn(&mut app, 2);
        app.world_mut().spawn(Client::new(new_conn));
        for line in ["Twinsie", "password"] {
            app.world_mut().write_message(ConnectionInput {
                connection: new_conn,
                text: line.into(),
            });
            app.update();
        }

        // Old session was told + disconnected.
        {
            let msgs = app.world().resource::<Messages<ConnectionOutput>>();
            let mut cursor = msgs.get_cursor();
            assert!(
                cursor
                    .read(msgs)
                    .any(|o| o.connection == old_conn && o.text.contains("Someone else")),
                "old session should be notified"
            );
        }
        let dc = app.world().resource::<Messages<DisconnectRequest>>();
        assert!(
            dc.get_cursor().read(dc).any(|d| d.connection == old_conn),
            "old session should be disconnected"
        );

        // Entity handed to the new connection; still exactly one entity.
        let mut q = app.world_mut().query::<(Entity, &GrimName)>();
        assert_eq!(
            q.iter(app.world())
                .filter(|(_, n)| n.0 == "Twinsie")
                .count(),
            1,
            "takeover must not duplicate the entity"
        );
        let mut pq = app.world_mut().query::<&Player>();
        assert_eq!(
            pq.get(app.world(), char_entity).unwrap().connection,
            new_conn
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ─── Character creation picker: gender → race → class ────

mod character_creation {
    use super::*;

    /// Read a connection's client state.
    fn state_of(app: &mut App, conn: Entity) -> ClientState {
        let mut q = app.world_mut().query::<&Client>();
        q.iter(app.world())
            .find(|c| c.connection == conn)
            .unwrap()
            .state
            .clone()
    }

    /// Boot an app in an isolated dir with an account already logged in and a
    /// client parked at `CreateCharacter`, ready to accept a name.
    fn app_at_create_character(dir: &std::path::Path) -> (App, Entity) {
        let mut app = test_app_in(dir);
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let account = Account {
            id: GrimId::new(),
            identifier: "maker@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![],
            created_at: Utc::now(),
        };
        let account_entity = app.world_mut().spawn(account).id();

        let conn = spawn_conn(&mut app, 1);
        let mut client = Client::new(conn);
        client.state = ClientState::CreateCharacter;
        client.account = Some(account_entity);
        app.world_mut().spawn(client);
        app.update(); // flush spawns
        (app, conn)
    }

    fn send(app: &mut App, conn: Entity, text: &str) {
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: text.into(),
        });
        app.update();
    }

    #[test]
    fn full_flow_transitions_and_persists_selections() {
        let dir = unique_dir("create-full");
        let (mut app, conn) = app_at_create_character(&dir);

        // Name → SelectGender
        send(&mut app, conn, "Aria");
        assert_eq!(
            state_of(&mut app, conn),
            ClientState::SelectGender {
                name: "Aria".into()
            }
        );

        // Gender (index 2 = Female) → SelectRace
        send(&mut app, conn, "2");
        assert_eq!(
            state_of(&mut app, conn),
            ClientState::SelectRace {
                name: "Aria".into(),
                gender: Gender::Female,
            }
        );

        // Race (slug prefix "dwa" → dwarf) → SelectClass
        send(&mut app, conn, "dwa");
        assert_eq!(
            state_of(&mut app, conn),
            ClientState::SelectClass {
                name: "Aria".into(),
                gender: Gender::Female,
                race: "dwarf".into(),
            }
        );

        // Class (name prefix "Mage") → MotdPrompt + persisted character.
        send(&mut app, conn, "Mage");
        assert_eq!(state_of(&mut app, conn), ClientState::MotdPrompt);

        let mut q = app.world_mut().query::<(&GrimName, &Actor, &Character)>();
        let (_, actor, ch) = q
            .iter(app.world())
            .find(|(n, _, _)| n.0 == "Aria")
            .expect("character spawned");
        assert_eq!(actor.gender, Gender::Female);
        assert_eq!(actor.race, "dwarf");
        assert_eq!(ch.class, "mage");
        assert_eq!(actor.level, 1);

        // And it round-trips through disk with those fields present.
        let path = dir.join("characters").join("Aria.json");
        let json = std::fs::read_to_string(&path).expect("character json on disk");
        let loaded: StoredCharacter = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.gender, Gender::Female);
        assert_eq!(loaded.race, "dwarf");
        assert_eq!(loaded.class, "mage");
        assert_eq!(loaded.level, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_gender_reprompts_without_advancing() {
        let dir = unique_dir("create-bad-gender");
        let (mut app, conn) = app_at_create_character(&dir);
        send(&mut app, conn, "Bran");
        // Out-of-range index → still SelectGender.
        send(&mut app, conn, "9");
        assert_eq!(
            state_of(&mut app, conn),
            ClientState::SelectGender {
                name: "Bran".into()
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tier_two_class_slug_is_not_creatable() {
        let dir = unique_dir("create-tier2");
        let (mut app, conn) = app_at_create_character(&dir);
        send(&mut app, conn, "Cade");
        send(&mut app, conn, "1"); // gender
        send(&mut app, conn, "human"); // race
                                       // "champion" is a tier-2 class → not in the creatable menu → rejected.
        send(&mut app, conn, "champion");
        assert!(matches!(
            state_of(&mut app, conn),
            ClientState::SelectClass { .. }
        ));
        // A tier-1 class is accepted.
        send(&mut app, conn, "warrior");
        assert_eq!(state_of(&mut app, conn), ClientState::MotdPrompt);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Write a character JSON to disk as if another session had created it. Uses
    /// race "elf" as a marker so a clobber is detectable.
    fn write_existing_character(dir: &std::path::Path, name: &str) {
        let existing = StoredCharacter {
            id: GrimId::new(),
            name: name.into(),
            account_id: GrimId::new(),
            created_at: Utc::now(),
            last_room: None,
            roles: vec![],
            gender: Gender::Neutral,
            race: "elf".into(),
            class: "warrior".into(),
            level: 1,
            title: None,
            restrings: std::collections::HashMap::new(),
        };
        std::fs::create_dir_all(dir.join("characters")).unwrap();
        std::fs::write(
            dir.join("characters").join(format!("{name}.json")),
            serde_json::to_string(&existing).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn name_already_taken_is_rejected_at_entry() {
        let dir = unique_dir("create-taken-entry");
        let (mut app, conn) = app_at_create_character(&dir);
        write_existing_character(&dir, "Taken");
        send(&mut app, conn, "Taken");
        // Rejected up front — no picker started.
        assert_eq!(state_of(&mut app, conn), ClientState::CreateCharacter);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn name_taken_during_picker_is_caught_at_finalize() {
        let dir = unique_dir("create-taken-finalize");
        let (mut app, conn) = app_at_create_character(&dir);
        send(&mut app, conn, "Racer"); // → SelectGender
        send(&mut app, conn, "1"); // gender → SelectRace
        send(&mut app, conn, "human"); // race → SelectClass
                                       // A rival session finalizes the same name mid-picker.
        write_existing_character(&dir, "Racer");
        send(&mut app, conn, "warrior"); // finalize hits the collision
        assert_eq!(state_of(&mut app, conn), ClientState::CreateCharacter);
        // The pre-existing character was NOT clobbered (still race "elf", not the
        // picker's "human").
        let json = std::fs::read_to_string(dir.join("characters").join("Racer.json")).unwrap();
        let loaded: StoredCharacter = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.race, "elf");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ─── Legacy character backfill ───────────

mod legacy_backfill {
    use super::*;

    fn send(app: &mut App, conn: Entity, text: &str) {
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: text.into(),
        });
        app.update();
    }

    fn state_of(app: &mut App, conn: Entity) -> ClientState {
        let mut q = app.world_mut().query::<&Client>();
        q.iter(app.world())
            .find(|c| c.connection == conn)
            .unwrap()
            .state
            .clone()
    }

    /// Boot an app whose (resident) account owns one on-disk character with the
    /// given race/class, log in, and stop at the character-select menu. An empty
    /// race/class models a legacy character; a filled one a normal character.
    fn app_at_menu_owning(
        dir: &std::path::Path,
        char_name: &str,
        race: &str,
        class: &str,
    ) -> (App, Entity) {
        let mut app = test_app_in(dir);
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let account_id = GrimId::new();
        let char_id = GrimId::new();
        app.world_mut().spawn(Account {
            id: account_id,
            identifier: "legacy@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![char_id],
            created_at: Utc::now(),
        });
        write_disk_char(
            dir,
            &StoredCharacter {
                id: char_id,
                name: char_name.into(),
                account_id,
                created_at: Utc::now(),
                last_room: None,
                roles: Vec::new(),
                gender: Gender::Neutral,
                race: race.into(),
                class: class.into(),
                level: 1,
                title: None,
                restrings: std::collections::HashMap::new(),
            },
        );

        let conn = spawn_conn(&mut app, 1);
        app.world_mut().spawn(Client::new(conn));
        send(&mut app, conn, "legacy@example.com");
        send(&mut app, conn, "password");
        (app, conn)
    }

    #[test]
    fn legacy_char_routes_through_picker_and_backfills_build() {
        let dir = unique_dir("legacy-backfill");
        let (mut app, conn) = app_at_menu_owning(&dir, "Oldtimer", "", "");
        assert_eq!(state_of(&mut app, conn), ClientState::CharacterSelect);

        // Selecting the legacy character routes to the picker, NOT the world.
        send(&mut app, conn, "1");
        assert_eq!(
            state_of(&mut app, conn),
            ClientState::SelectGender {
                name: "Oldtimer".into()
            }
        );

        // Complete the picker: gender index 2 = Female, race + class by slug.
        send(&mut app, conn, "2");
        send(&mut app, conn, "dwarf");
        send(&mut app, conn, "mage");
        // Backfill entered the world via the normal path → MOTD.
        assert_eq!(state_of(&mut app, conn), ClientState::MotdPrompt);

        // The chosen build is persisted; id/account/level are preserved (level
        // still 1 — no XP system).
        let json = std::fs::read_to_string(dir.join("characters").join("Oldtimer.json")).unwrap();
        let loaded: StoredCharacter = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.gender, Gender::Female);
        assert_eq!(loaded.race, "dwarf");
        assert_eq!(loaded.class, "mage");
        assert_eq!(loaded.level, 1);

        // Exactly one in-world entity — backfill reused the normal world-entry
        // (spawn-from-disk) path, it did not duplicate the character.
        let mut q = app.world_mut().query::<(&Actor, &Character, &GrimName)>();
        assert_eq!(
            q.iter(app.world())
                .filter(|(_, _, n)| n.0 == "Oldtimer")
                .count(),
            1,
            "backfill must not spawn a duplicate character entity"
        );
        let (actor, character, _) = q
            .iter(app.world())
            .find(|(_, _, n)| n.0 == "Oldtimer")
            .unwrap();
        assert_eq!(actor.race, "dwarf");
        assert_eq!(character.class, "mage");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normal_char_enters_directly_without_picker() {
        let dir = unique_dir("legacy-normal");
        let (mut app, conn) = app_at_menu_owning(&dir, "Freshie", "human", "warrior");
        assert_eq!(state_of(&mut app, conn), ClientState::CharacterSelect);

        // A fully-built character enters the world directly — the picker is
        // never shown (behavior-preserving for the normal path).
        send(&mut app, conn, "1");
        assert_eq!(state_of(&mut app, conn), ClientState::MotdPrompt);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_name_login_routes_to_picker() {
        // Logging in directly by CHARACTER NAME (bypassing the select menu) must
        // also route a legacy character through the picker, not straight into the
        // world.
        let dir = unique_dir("legacy-name-login");
        let mut app = test_app_in(&dir);
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let account_id = GrimId::new();
        let char_id = GrimId::new();
        app.world_mut().spawn(Account {
            id: account_id,
            identifier: "legacy@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![char_id],
            created_at: Utc::now(),
        });
        write_disk_char(
            &dir,
            &StoredCharacter {
                id: char_id,
                name: "Oldtimer".into(),
                account_id,
                created_at: Utc::now(),
                last_room: None,
                roles: Vec::new(),
                gender: Gender::Neutral,
                race: String::new(),
                class: String::new(),
                level: 1,
                title: None,
                restrings: std::collections::HashMap::new(),
            },
        );

        let conn = spawn_conn(&mut app, 1);
        app.world_mut().spawn(Client::new(conn));
        // Log in by character name, then password.
        send(&mut app, conn, "Oldtimer");
        send(&mut app, conn, "password");
        assert_eq!(
            state_of(&mut app, conn),
            ClientState::SelectGender {
                name: "Oldtimer".into()
            }
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn backfill_refreshes_resident_linkdead_entity() {
        // A legacy character that is still RESIDENT (linkdead after a crash) must
        // have its live ECS Character refreshed by the backfill, not just its disk
        // copy — else a later move/disconnect save re-clobbers the JSON.
        let dir = unique_dir("legacy-resident");
        let mut app = test_app_in(&dir);
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let account_id = GrimId::new();
        let char_id = GrimId::new();
        app.world_mut().spawn(Account {
            id: account_id,
            identifier: "legacy@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![char_id],
            created_at: Utc::now(),
        });
        let legacy = StoredCharacter {
            id: char_id,
            name: "Ghost".into(),
            account_id,
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
            gender: Gender::Neutral,
            race: String::new(),
            class: String::new(),
            level: 1,
            title: None,
            restrings: std::collections::HashMap::new(),
        };
        write_disk_char(&dir, &legacy);
        let (name, actor, character) = legacy.into_components();
        // Resident but linkdead (as after a crash): entity exists, empty build.
        let entity = app
            .world_mut()
            .spawn((
                name,
                actor,
                character,
                Description("A faded soul.".into()),
                InRoom { room },
                Linkdead,
                OutputHistory::with_max(100),
            ))
            .id();

        let conn = spawn_conn(&mut app, 1);
        app.world_mut().spawn(Client::new(conn));
        send(&mut app, conn, "legacy@example.com");
        send(&mut app, conn, "password"); // → CharacterSelect
        send(&mut app, conn, "1"); // select Ghost → legacy picker
        send(&mut app, conn, "2"); // gender
        send(&mut app, conn, "elf"); // race
        send(&mut app, conn, "cleric"); // class → backfill + reconnect

        // The live entity now carries the picked build (not the stale empty one).
        let actor = app
            .world()
            .get::<Actor>(entity)
            .expect("resident entity still present");
        let ch = app
            .world()
            .get::<Character>(entity)
            .expect("resident entity still present");
        assert_eq!(actor.race, "elf");
        assert_eq!(ch.class, "cleric");
        // No duplicate entity was spawned.
        let mut q = app.world_mut().query::<&GrimName>();
        assert_eq!(
            q.iter(app.world()).filter(|n| n.0 == "Ghost").count(),
            1,
            "backfill must refresh the resident entity, not duplicate it"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
