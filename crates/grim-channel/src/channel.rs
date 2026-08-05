use grim_text::tr;

use bevy::prelude::*;
use grim_engine_types::components::{Character, InRoom, LastWhisperFrom, Name, Player, Room};
use grim_engine_types::events::{
    Command, EngineCommand, GlobalEcho, InfoMessage, OocEvent, SayEvent, YellEvent,
};

/// Handles `say` commands, emitting a `SayEvent` for room broadcast and an
/// `InfoMessage` echo back to the speaker.
pub struct ChannelPlugin;

impl Plugin for ChannelPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EngineCommand>()
            .add_message::<InfoMessage>()
            .add_message::<SayEvent>()
            .add_message::<YellEvent>()
            .add_message::<OocEvent>()
            .add_message::<GlobalEcho>()
            .add_systems(
                Update,
                (
                    handle_say,
                    handle_yell,
                    handle_ooc,
                    handle_gecho,
                    handle_tell,
                    handle_reply,
                ),
            );
    }
}

/// Deliver one whisper: echo `You tell <Name> '<text>'` to the sender, and —
/// for a distinct recipient — `<Sender> tells you '<text>'` plus record the
/// sender as the recipient's [`LastWhisperFrom`] so they can `reply`. A whisper
/// to `self` echoes only the "You tell …" line.
fn deliver_whisper(
    actor: Entity,
    recipient: Entity,
    text: &str,
    names: &Query<&Name>,
    info: &mut MessageWriter<InfoMessage>,
    commands: &mut Commands,
) {
    let recipient_name = names
        .get(recipient)
        .map(|n| n.0.clone())
        .unwrap_or_default();
    info.write(InfoMessage {
        target: actor,
        text: format!("You tell {recipient_name} '{text}'\n"),
    });
    if recipient != actor {
        let sender_name = names.get(actor).map(|n| n.0.clone()).unwrap_or_default();
        info.write(InfoMessage {
            target: recipient,
            text: format!("{sender_name} tells you '{text}'\n"),
        });
        commands.entity(recipient).insert(LastWhisperFrom(actor));
    }
}

/// `tell <target> <text>` (alias `whisper`): a private message to one player.
/// `target` is fuzzy-matched (case-insensitive name prefix) among connected
/// players; `self` targets the sender.
fn handle_tell(
    mut engine: MessageReader<EngineCommand>,
    players: Query<(Entity, &Name, &Player)>,
    names: Query<&Name>,
    mut info: MessageWriter<InfoMessage>,
    mut commands: Commands,
) {
    for cmd in engine.read() {
        let Command::Tell { target, text } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;

        let recipient = if target.eq_ignore_ascii_case("self") {
            Some(actor)
        } else {
            let want = target.to_ascii_lowercase();
            // Match any player in the world, including linkdead ones — a whisper
            // to a linkdead player is fine; they'll see it when they return.
            players
                .iter()
                .find(|(_, n, _)| n.0.to_ascii_lowercase().starts_with(&want))
                .map(|(e, _, _)| e)
        };

        let Some(recipient) = recipient else {
            info.write(InfoMessage {
                target: actor,
                text: format!("No one named '{target}' is here to tell.\n"),
            });
            continue;
        };

        deliver_whisper(actor, recipient, text, &names, &mut info, &mut commands);
    }
}

/// `reply <text>`: whisper the last player who whispered you
/// ([`LastWhisperFrom`]). Fails gracefully if no one has, or if they've left.
fn handle_reply(
    mut engine: MessageReader<EngineCommand>,
    last: Query<&LastWhisperFrom>,
    players: Query<&Player>,
    names: Query<&Name>,
    mut info: MessageWriter<InfoMessage>,
    mut commands: Commands,
) {
    for cmd in engine.read() {
        let Command::Reply { text } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;

        let Ok(&LastWhisperFrom(target)) = last.get(actor) else {
            info.write(InfoMessage {
                target: actor,
                text: "You have no one to reply to.\n".into(),
            });
            continue;
        };
        // Linkdead is fine (they'll see it on return); only a target that has
        // fully left the world (no longer a player entity) can't be replied to.
        if players.get(target).is_err() {
            info.write(InfoMessage {
                target: actor,
                text: "They are no longer here to reply to.\n".into(),
            });
            continue;
        }

        deliver_whisper(actor, target, text, &names, &mut info, &mut commands);
    }
}

/// `say <text>`: broadcast to the room and echo "You say, '<text>'" to the actor.
fn handle_say(
    mut engine: MessageReader<EngineCommand>,
    inroom: Query<(&InRoom, &Name)>,
    mut say: MessageWriter<SayEvent>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Say { text } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Ok((ir, _name)) = inroom.get(actor) else {
            continue;
        };
        say.write(SayEvent {
            room: ir.room,
            actor,
            text: text.clone(),
        });
        info.write(InfoMessage {
            target: actor,
            text: tr!("social.say.first_party", text = text),
        });
    }
}

fn handle_yell(
    mut engine: MessageReader<EngineCommand>,
    inroom: Query<(&InRoom, &Name)>,
    rooms: Query<&Room>,
    mut yell: MessageWriter<YellEvent>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Yell { text } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Ok((ir, _name)) = inroom.get(actor) else {
            continue;
        };
        let Ok(room) = rooms.get(ir.room) else {
            continue;
        };
        yell.write(YellEvent {
            area: room.area,
            actor,
            text: text.clone(),
        });
        info.write(InfoMessage {
            target: actor,
            text: format!("You yell, '{}'\n", text),
        });
    }
}

fn handle_ooc(
    mut engine: MessageReader<EngineCommand>,
    mut ooc: MessageWriter<OocEvent>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Ooc { text } = &cmd.command else {
            continue;
        };
        ooc.write(OocEvent {
            actor: cmd.client,
            text: text.clone(),
        });
        info.write(InfoMessage {
            target: cmd.client,
            text: format!("[OOC] You: {}\n", text),
        });
    }
}

/// Handles admin `gecho`, emitting a `GlobalEcho` for world-wide broadcast.
/// Admin gating happens at dispatch (see the grim-scene dispatcher); by the
/// time it reaches here the command is authorized. Rendering — including
/// per-recipient attribution — is `format_output`'s job.
fn handle_gecho(
    mut engine: MessageReader<EngineCommand>,
    mut echo: MessageWriter<GlobalEcho>,
    characters: Query<&Character>,
) {
    for cmd in engine.read() {
        let Command::Gecho { text } = &cmd.command else {
            continue;
        };
        // Defense in depth, mirroring `handle_goto` / `handle_shutdown_command`.
        // Dispatch already masks `gecho` as unknown for non-admins, so a
        // well-behaved session never sends this for a non-admin. If one arrives
        // from another command source, fail closed and stay silent.
        let is_admin = characters
            .get(cmd.client)
            .map(Character::is_admin)
            .unwrap_or(false);
        if !is_admin {
            continue;
        }
        echo.write(GlobalEcho {
            actor: cmd.client,
            text: text.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grim_engine_types::components::{Character, Gender, InRoom, Name, Player, Role, Room};
    use grim_engine_types::events::{
        Command, EngineCommand, GlobalEcho, InfoMessage, OocEvent, SayEvent, YellEvent,
    };
    use grim_engine_types::GrimId;

    use chrono::Utc;

    fn spawn_admin(app: &mut App) -> Entity {
        app.world_mut()
            .spawn(Character {
                id: GrimId::new(),
                name: "Admin".into(),
                account_id: GrimId::new(),
                created_at: Utc::now(),
                last_room: None,
                roles: vec![Role::Admin],
                gender: Gender::Neutral,
                race: String::new(),
                class: String::new(),
                level: 1,
            })
            .id()
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(ChannelPlugin);
        app
    }

    /// (target, text) of every InfoMessage emitted.
    fn infos(app: &App) -> Vec<(Entity, String)> {
        let m = app.world().resource::<Messages<InfoMessage>>();
        let mut c = m.get_cursor();
        c.read(m).map(|i| (i.target, i.text.clone())).collect()
    }

    fn spawn_player(app: &mut App, name: &str) -> Entity {
        app.world_mut()
            .spawn((
                Name(name.into()),
                Player {
                    connection: Some(Entity::PLACEHOLDER),
                },
            ))
            .id()
    }

    #[test]
    fn tell_fuzzy_matches_and_messages_both_parties() {
        let mut app = test_app();
        let alice = spawn_player(&mut app, "Alice");
        let wrack = spawn_player(&mut app, "Wrack");
        app.world_mut().write_message(EngineCommand {
            client: alice,
            command: Command::Tell {
                target: "wr".into(), // case-insensitive prefix
                text: "hi".into(),
            },
        });
        app.update();
        let msgs = infos(&app);
        assert!(msgs.contains(&(alice, "You tell Wrack 'hi'\n".to_string())));
        assert!(msgs.contains(&(wrack, "Alice tells you 'hi'\n".to_string())));
        assert_eq!(msgs.len(), 2);
    }

    #[test]
    fn tell_self_only_echoes_to_sender() {
        let mut app = test_app();
        let alice = spawn_player(&mut app, "Alice");
        app.world_mut().write_message(EngineCommand {
            client: alice,
            command: Command::Tell {
                target: "self".into(),
                text: "note".into(),
            },
        });
        app.update();
        let msgs = infos(&app);
        assert_eq!(msgs, vec![(alice, "You tell Alice 'note'\n".to_string())]);
    }

    #[test]
    fn tell_unknown_target_reports_to_sender() {
        let mut app = test_app();
        let alice = spawn_player(&mut app, "Alice");
        app.world_mut().write_message(EngineCommand {
            client: alice,
            command: Command::Tell {
                target: "nobody".into(),
                text: "hi".into(),
            },
        });
        app.update();
        let msgs = infos(&app);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].0, alice);
        assert!(msgs[0].1.contains("No one named 'nobody'"));
    }

    #[test]
    fn tell_reaches_linkdead_player() {
        // A linkdead player is still in the world and can be told; they'll see
        // it when they return.
        let mut app = test_app();
        let alice = spawn_player(&mut app, "Alice");
        let bob = app
            .world_mut()
            .spawn((Name("Bob".into()), Player { connection: None }))
            .id();
        app.world_mut().write_message(EngineCommand {
            client: alice,
            command: Command::Tell {
                target: "Bob".into(),
                text: "hi".into(),
            },
        });
        app.update();
        let msgs = infos(&app);
        assert!(msgs.contains(&(alice, "You tell Bob 'hi'\n".to_string())));
        assert!(msgs.contains(&(bob, "Alice tells you 'hi'\n".to_string())));
    }

    #[test]
    fn reply_answers_last_whisperer() {
        let mut app = test_app();
        let alice = spawn_player(&mut app, "Alice");
        let bob = spawn_player(&mut app, "Bob");
        // Alice whispers Bob → sets Bob's LastWhisperFrom = Alice.
        app.world_mut().write_message(EngineCommand {
            client: alice,
            command: Command::Tell {
                target: "Bob".into(),
                text: "hi".into(),
            },
        });
        app.update(); // flush the deferred LastWhisperFrom insert
        app.world_mut().write_message(EngineCommand {
            client: bob,
            command: Command::Reply { text: "hey".into() },
        });
        app.update();
        let msgs = infos(&app);
        assert!(msgs.contains(&(bob, "You tell Alice 'hey'\n".to_string())));
        assert!(msgs.contains(&(alice, "Bob tells you 'hey'\n".to_string())));
    }

    #[test]
    fn reply_with_no_prior_whisper_is_reported() {
        let mut app = test_app();
        let alice = spawn_player(&mut app, "Alice");
        app.world_mut().write_message(EngineCommand {
            client: alice,
            command: Command::Reply { text: "hi".into() },
        });
        app.update();
        let msgs = infos(&app);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].1.contains("no one to reply to"));
    }

    #[test]
    fn reply_to_departed_player_is_reported() {
        let mut app = test_app();
        let alice = spawn_player(&mut app, "Alice");
        let bob = spawn_player(&mut app, "Bob");
        // Bob whispers Alice → Alice's LastWhisperFrom = Bob.
        app.world_mut().write_message(EngineCommand {
            client: bob,
            command: Command::Tell {
                target: "Alice".into(),
                text: "hi".into(),
            },
        });
        app.update();
        // Bob fully leaves the world (quit → despawn).
        app.world_mut().despawn(bob);
        app.world_mut().write_message(EngineCommand {
            client: alice,
            command: Command::Reply {
                text: "you there?".into(),
            },
        });
        app.update();
        let msgs = infos(&app);
        assert!(msgs
            .iter()
            .any(|(t, txt)| *t == alice && txt.contains("no longer here")));
    }

    #[test]
    fn say_emits_say_event_and_info_message() {
        let mut app = test_app();
        let room = app.world_mut().spawn(()).id();
        let actor = app
            .world_mut()
            .spawn((InRoom { room }, Name("hero".into())))
            .id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Say { text: "hi".into() },
        });
        app.update();
        {
            let messages = app.world().resource::<Messages<SayEvent>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one SayEvent");
            assert_eq!(ev.room, room);
            assert_eq!(ev.actor, actor);
            assert_eq!(ev.text, "hi");
            assert!(iter.next().is_none(), "expected exactly one SayEvent");
        }
        {
            let messages = app.world().resource::<Messages<InfoMessage>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one InfoMessage");
            assert_eq!(ev.target, actor);
            assert_eq!(ev.text, "@xf0fYou say @r'@x909hi@r'\n");
            assert!(iter.next().is_none(), "expected exactly one InfoMessage");
        }
    }

    #[test]
    fn yell_emits_yell_event_and_info_message() {
        let mut app = test_app();
        let area = app.world_mut().spawn(()).id();
        let room = app
            .world_mut()
            .spawn(Room {
                id: GrimId::new(),
                friendly_id: "test".into(),
                name: "Test Room".into(),
                description: "".into(),
                area,
            })
            .id();
        let actor = app
            .world_mut()
            .spawn((InRoom { room }, Name("hero".into())))
            .id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Yell {
                text: "help!".into(),
            },
        });
        app.update();
        {
            let messages = app.world().resource::<Messages<YellEvent>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one YellEvent");
            assert_eq!(ev.area, area);
            assert_eq!(ev.actor, actor);
            assert_eq!(ev.text, "help!");
            assert!(iter.next().is_none(), "expected exactly one YellEvent");
        }
        {
            let messages = app.world().resource::<Messages<InfoMessage>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one InfoMessage");
            assert_eq!(ev.target, actor);
            assert_eq!(ev.text, "You yell, 'help!'\n");
            assert!(iter.next().is_none(), "expected exactly one InfoMessage");
        }
    }

    #[test]
    fn ooc_emits_ooc_event_and_info_message() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(()).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Ooc {
                text: "hello everyone".into(),
            },
        });
        app.update();
        {
            let messages = app.world().resource::<Messages<OocEvent>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one OocEvent");
            assert_eq!(ev.actor, actor);
            assert_eq!(ev.text, "hello everyone");
            assert!(iter.next().is_none(), "expected exactly one OocEvent");
        }
        {
            let messages = app.world().resource::<Messages<InfoMessage>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one InfoMessage");
            assert_eq!(ev.target, actor);
            assert_eq!(ev.text, "[OOC] You: hello everyone\n");
            assert!(iter.next().is_none(), "expected exactly one InfoMessage");
        }
    }

    #[test]
    fn gecho_emits_global_echo() {
        let mut app = test_app();
        let actor = spawn_admin(&mut app);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Gecho {
                text: "server reboot soon".into(),
            },
        });
        app.update();
        let messages = app.world().resource::<Messages<GlobalEcho>>();
        let mut cursor = messages.get_cursor();
        let mut iter = cursor.read(messages);
        let ev = iter.next().expect("expected one GlobalEcho");
        assert_eq!(ev.actor, actor);
        assert_eq!(ev.text, "server reboot soon");
        assert!(iter.next().is_none(), "expected exactly one GlobalEcho");
    }

    #[test]
    fn gecho_from_non_admin_is_ignored() {
        // Defense in depth: even if a `Gecho` reaches the handler for a
        // non-admin (no `Character`/no admin role), no `GlobalEcho` is emitted.
        let mut app = test_app();
        let actor = app.world_mut().spawn(()).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Gecho {
                text: "should not broadcast".into(),
            },
        });
        app.update();
        let messages = app.world().resource::<Messages<GlobalEcho>>();
        let mut cursor = messages.get_cursor();
        assert!(
            cursor.read(messages).next().is_none(),
            "non-admin gecho must emit no GlobalEcho"
        );
    }

    #[test]
    fn test_say_without_inroom_does_nothing() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(()).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Say {
                text: "hello".into(),
            },
        });
        app.update();
        // No SayEvent should be emitted
        let messages = app.world().resource::<Messages<SayEvent>>();
        let mut cursor = messages.get_cursor();
        assert_eq!(cursor.read(messages).count(), 0, "No SayEvent expected");
        // No InfoMessage either
        let info = app.world().resource::<Messages<InfoMessage>>();
        let mut cursor = info.get_cursor();
        assert_eq!(cursor.read(info).count(), 0, "No InfoMessage expected");
    }

    #[test]
    fn test_yell_without_inroom_does_nothing() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(()).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Yell {
                text: "help!".into(),
            },
        });
        app.update();
        // No YellEvent should be emitted
        let messages = app.world().resource::<Messages<YellEvent>>();
        let mut cursor = messages.get_cursor();
        assert_eq!(cursor.read(messages).count(), 0, "No YellEvent expected");
        // No InfoMessage either
        let info = app.world().resource::<Messages<InfoMessage>>();
        let mut cursor = info.get_cursor();
        assert_eq!(cursor.read(info).count(), 0, "No InfoMessage expected");
    }
}
