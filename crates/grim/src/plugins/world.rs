use crate::components::{Exits, InRoom, Name, Player};
use crate::events::{
    Command, DisconnectRequest, EngineCommand, InfoMessage, LookEntity, LookRoom, MoveEvent,
};
use bevy::prelude::*;

/// Handles `look`, `move`, and `quit` commands; emits room/entity description
/// events, movement broadcasts, and disconnect requests.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EngineCommand>()
            .add_message::<LookRoom>()
            .add_message::<LookEntity>()
            .add_message::<MoveEvent>()
            .add_message::<InfoMessage>()
            .add_message::<DisconnectRequest>()
            .add_systems(Update, (handle_look, handle_move, handle_quit));
    }
}

/// `look` / `look <target>`: show the actor's room or a named entity in it.
fn handle_look(
    mut engine: MessageReader<EngineCommand>,
    inroom: Query<&InRoom>,
    named: Query<(Entity, &InRoom, &Name)>,
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
                let subject = named
                    .iter()
                    .find(|(_, ir, nm)| ir.room == room && nm.0.to_lowercase() == want)
                    .map(|(e, _, _)| e);
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

/// `move <direction>`: traverse an exit, emitting a movement event and an
/// automatic look at the destination.
fn handle_move(
    mut engine: MessageReader<EngineCommand>,
    mut inroom: Query<&mut InRoom>,
    exits: Query<&Exits>,
    mut move_ev: MessageWriter<MoveEvent>,
    mut look_room: MessageWriter<LookRoom>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Move { direction } = cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let from = match inroom.get(actor) {
            Ok(ir) => ir.room,
            Err(_) => continue,
        };
        match exits.get(from) {
            Ok(room_exits) => match room_exits.exits.get(&direction).copied() {
                Some(to) => {
                    if let Ok(mut ir) = inroom.get_mut(actor) {
                        ir.room = to;
                    }
                    move_ev.write(MoveEvent {
                        actor,
                        from,
                        to,
                        direction,
                    });
                    look_room.write(LookRoom {
                        target: actor,
                        room: to,
                    });
                }
                None => {
                    info.write(InfoMessage {
                        target: actor,
                        text: "You can't go that way.\n".into(),
                    });
                }
            },
            Err(_) => {
                info.write(InfoMessage {
                    target: actor,
                    text: "You can't go that way.\n".into(),
                });
            }
        }
    }
}

/// `quit`: request a clean disconnect of the actor's underlying connection.
fn handle_quit(
    mut engine: MessageReader<EngineCommand>,
    players: Query<&Player>,
    mut disconnect: MessageWriter<DisconnectRequest>,
) {
    for cmd in engine.read() {
        let Command::Quit = cmd.command else {
            continue;
        };
        if let Ok(player) = players.get(cmd.client) {
            if let Some(conn) = player.connection {
                disconnect.write(DisconnectRequest { connection: conn });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cardinal::Cardinal;
    use crate::components::{Exits, InRoom, Name};
    use crate::events::{Command, EngineCommand, InfoMessage, LookEntity, LookRoom, MoveEvent};

    macro_rules! count_messages {
        ($app:expr, $t:ty) => {{
            let messages = $app.world().resource::<Messages<$t>>();
            let mut cursor = messages.get_cursor();
            cursor.read(messages).count()
        }};
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(WorldPlugin);
        app
    }

    fn look_room_count(app: &App) -> usize {
        count_messages!(app, LookRoom)
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
        {
            let messages = app.world().resource::<Messages<LookRoom>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one LookRoom");
            assert_eq!(ev.target, actor);
            assert_eq!(ev.room, room);
            assert!(iter.next().is_none(), "expected exactly one LookRoom");
        }
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
        {
            let messages = app.world().resource::<Messages<LookEntity>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one LookEntity");
            assert_eq!(ev.target, actor);
            assert_eq!(ev.subject, goblin);
            assert!(iter.next().is_none(), "expected exactly one LookEntity");
        }
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

    #[test]
    fn move_valid_exit_updates_in_room() {
        let mut app = test_app();
        let room2 = app.world_mut().spawn(()).id();
        let mut exits = Exits::default();
        exits.exits.insert(Cardinal::North, room2);
        let room1 = app.world_mut().spawn(exits).id();
        let actor = app.world_mut().spawn(InRoom { room: room1 }).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Move {
                direction: Cardinal::North,
            },
        });
        app.update();
        assert_eq!(app.world().get::<InRoom>(actor).unwrap().room, room2);
        {
            let messages = app.world().resource::<Messages<MoveEvent>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one MoveEvent");
            assert_eq!(ev.from, room1);
            assert_eq!(ev.to, room2);
            assert!(iter.next().is_none(), "expected exactly one MoveEvent");
        }
        assert_eq!(look_room_count(&app), 1);
    }

    #[test]
    fn move_no_exit_emits_info_message() {
        let mut app = test_app();
        let room1 = app.world_mut().spawn(Exits::default()).id();
        let actor = app.world_mut().spawn(InRoom { room: room1 }).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Move {
                direction: Cardinal::North,
            },
        });
        app.update();
        assert_eq!(app.world().get::<InRoom>(actor).unwrap().room, room1);
        {
            let messages = app.world().resource::<Messages<InfoMessage>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one InfoMessage");
            assert_eq!(ev.text, "You can't go that way.\n");
            assert!(iter.next().is_none(), "expected exactly one InfoMessage");
        }
    }
}
