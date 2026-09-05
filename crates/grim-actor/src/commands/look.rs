//! `look` rendering: turn a `look` command into the room/entity description
//! events (`LookRoom`/`LookEntity`) or a "not here" info message. Reads the
//! actor's [`InRoom`] placement and the [`Name`] of entities sharing the room;
//! creatures also match a [`Keywords`] prefix (`look smi` finds `smith`).

use bevy::prelude::*;
use grim_core::components::{Keywords, Name};
use grim_core::events::{Command, EngineCommand, InfoMessage, LookEntity, LookRoom};

use crate::placement::InRoom;

/// `look` / `look <target>`: show the actor's room or a named entity in it.
pub(crate) fn handle_look(
    mut engine: MessageReader<EngineCommand>,
    inroom: Query<&InRoom>,
    named: Query<(Entity, &InRoom, &Name, Option<&Keywords>)>,
    mut look_room: MessageWriter<LookRoom>,
    mut look_entity: MessageWriter<LookEntity>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Look { target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Ok(actor_room) = inroom.get(actor) else {
            continue;
        };
        match target {
            None => {
                look_room.write(LookRoom {
                    target: actor,
                    room: actor_room.room,
                });
            }
            Some(name) => {
                let want = name.to_lowercase();
                let room = actor_room.room;
                // `look self` always means your own character.
                let subject = if want == "self" {
                    Some(actor)
                } else {
                    named
                        .iter()
                        .find(|(_, ir, nm, kw)| {
                            ir.room == room
                                && !want.is_empty()
                                && (nm.0.to_lowercase() == want
                                    || kw.is_some_and(|kw| {
                                        kw.0.iter().any(|k| k.to_lowercase().starts_with(&want))
                                    }))
                        })
                        .map(|(e, _, _, _)| e)
                };
                match subject {
                    Some(subject) => {
                        look_entity.write(LookEntity {
                            target: actor,
                            subject,
                        });
                    }
                    None => {
                        info.write(InfoMessage {
                            target: actor,
                            text: "You don't see that here.\n".into(),
                        });
                    }
                }
            }
        }
    }
}

/// Wire the `look` handler and the input/delivery messages it owns. The
/// world-happening events it emits (`LookRoom`/`LookEntity`) are registered by
/// `grim_world::WorldPlugin`.
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_systems(Update, handle_look);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(grim_world::WorldPlugin);
        register(&mut app);
        app
    }

    fn look_room_count(app: &App) -> usize {
        let messages = app.world().resource::<Messages<LookRoom>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).count()
    }

    #[test]
    fn look_no_target_emits_look_room() {
        let mut app = test_app();
        let room = app.world_mut().spawn(()).id();
        let actor = app.world_mut().spawn(InRoom { room }).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Look { target: None },
        });
        app.update();
        let messages = app.world().resource::<Messages<LookRoom>>();
        let mut cursor = messages.get_cursor();
        let mut iter = cursor.read(messages);
        let ev = iter.next().expect("expected one LookRoom");
        assert_eq!(ev.target, actor);
        assert_eq!(ev.room, room);
        assert!(iter.next().is_none(), "expected exactly one LookRoom");
    }

    #[test]
    fn look_valid_target_emits_look_entity() {
        let mut app = test_app();
        let room = app.world_mut().spawn(()).id();
        let actor = app
            .world_mut()
            .spawn((InRoom { room }, Name("hero".into())))
            .id();
        let goblin = app
            .world_mut()
            .spawn((InRoom { room }, Name("goblin".into())))
            .id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Look {
                target: Some("goblin".into()),
            },
        });
        app.update();
        let messages = app.world().resource::<Messages<LookEntity>>();
        let mut cursor = messages.get_cursor();
        let mut iter = cursor.read(messages);
        let ev = iter.next().expect("expected one LookEntity");
        assert_eq!(ev.target, actor);
        assert_eq!(ev.subject, goblin);
        assert!(iter.next().is_none(), "expected exactly one LookEntity");
    }

    #[test]
    fn look_self_targets_own_character() {
        let mut app = test_app();
        let room = app.world_mut().spawn(()).id();
        let actor = app
            .world_mut()
            .spawn((InRoom { room }, Name("hero".into())))
            .id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Look {
                target: Some("self".into()),
            },
        });
        app.update();
        let messages = app.world().resource::<Messages<LookEntity>>();
        let mut cursor = messages.get_cursor();
        let mut iter = cursor.read(messages);
        let ev = iter.next().expect("expected one LookEntity");
        assert_eq!(ev.target, actor);
        assert_eq!(ev.subject, actor);
        assert!(iter.next().is_none(), "expected exactly one LookEntity");
    }

    #[test]
    fn look_keyword_matches_creature() {
        let mut app = test_app();
        let room = app.world_mut().spawn(()).id();
        let actor = app
            .world_mut()
            .spawn((InRoom { room }, Name("hero".into())))
            .id();
        let smith = app
            .world_mut()
            .spawn((
                InRoom { room },
                Name("Grimmok Ironhand".into()),
                Keywords(vec!["grimmok".into(), "smith".into()]),
            ))
            .id();
        for target in ["SMITH", "smi"] {
            app.world_mut().write_message(EngineCommand {
                client: actor,
                command: Command::Look {
                    target: Some(target.into()),
                },
            });
        }
        app.update();
        let messages = app.world().resource::<Messages<LookEntity>>();
        let mut cursor = messages.get_cursor();
        let mut iter = cursor.read(messages);
        for _ in 0..2 {
            let ev = iter.next().expect("expected one LookEntity");
            assert_eq!(ev.target, actor);
            assert_eq!(ev.subject, smith);
        }
        assert!(iter.next().is_none(), "expected exactly two LookEntity");
    }

    #[test]
    fn look_without_in_room_is_ignored() {
        // An actor with no `InRoom` placement produces no look output.
        let mut app = test_app();
        let actor = app.world_mut().spawn(()).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Look { target: None },
        });
        app.update();
        assert_eq!(look_room_count(&app), 0);
        let messages = app.world().resource::<Messages<InfoMessage>>();
        let mut cursor = messages.get_cursor();
        assert_eq!(cursor.read(messages).count(), 0);
    }

    #[test]
    fn look_invalid_target_emits_info_message() {
        let mut app = test_app();
        let room = app.world_mut().spawn(()).id();
        let actor = app
            .world_mut()
            .spawn((InRoom { room }, Name("hero".into())))
            .id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Look {
                target: Some("ghost".into()),
            },
        });
        app.update();
        {
            let messages = app.world().resource::<Messages<InfoMessage>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one InfoMessage");
            assert_eq!(ev.target, actor);
            assert_eq!(ev.text, "You don't see that here.\n");
            assert!(iter.next().is_none(), "expected exactly one InfoMessage");
        }
        assert_eq!(look_room_count(&app), 0);
    }
}
