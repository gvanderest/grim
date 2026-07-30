use bevy::prelude::*;
use grim_engine_types::components::{
    Area, Character, Exits, InRoom, Name, Player, Room, RoomLocation,
};
use grim_engine_types::events::{
    Command, EngineCommand, InfoMessage, LookEntity, LookRoom, MoveEvent,
};
use grim_networking::DisconnectRequest;

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

/// Resolve a room entity to its stable, entity-independent storage location
/// (area + room `friendly_id`s). These survive a world reseed, so persisting
/// them lets a character be placed back into the *new* instance of the same room
/// after a restart or copyover — see `grim-scene`'s placement resolver.
pub fn room_location(
    room: Entity,
    rooms: &Query<&Room>,
    areas: &Query<&Area>,
) -> Option<RoomLocation> {
    let r = rooms.get(room).ok()?;
    let area = areas.get(r.area).ok()?;
    Some(RoomLocation {
        area: area.friendly_id.clone(),
        room: r.friendly_id.clone(),
    })
}

/// `move <direction>`: traverse an exit, emitting a movement event and an
/// automatic look at the destination. Also refreshes the character's persisted
/// `last_room` so a restart/copyover resumes them where they walked to.
#[allow(clippy::too_many_arguments)]
fn handle_move(
    mut engine: MessageReader<EngineCommand>,
    mut inroom: Query<&mut InRoom>,
    exits: Query<&Exits>,
    rooms: Query<&Room>,
    areas: Query<&Area>,
    mut characters: Query<&mut Character>,
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
                    // Keep the persisted location current on every step so an
                    // unexpected restart or copyover resumes the character in the
                    // room they actually walked to, not a stale one.
                    if let Ok(mut character) = characters.get_mut(actor) {
                        if let Some(loc) = room_location(to, &rooms, &areas) {
                            character.last_room = Some(loc);
                        }
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
    use grim_engine_types::cardinal::Cardinal;
    use grim_engine_types::components::{Exits, InRoom, Name};
    use grim_engine_types::events::{
        Command, EngineCommand, InfoMessage, LookEntity, LookRoom, MoveEvent,
    };

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

    /// Spawn an area + a room in it, returning the room entity. `friendly_id`s
    /// are the stable storage keys `last_room` records.
    fn spawn_room(app: &mut App, area_fid: &str, room_fid: &str, exits: Exits) -> Entity {
        use grim_engine_types::components::{Area, Room};
        use uuid::Uuid;
        let area = app
            .world_mut()
            .spawn(Area {
                id: Uuid::new_v4(),
                friendly_id: area_fid.into(),
                name: area_fid.into(),
            })
            .id();
        app.world_mut()
            .spawn((
                Room {
                    id: Uuid::new_v4(),
                    friendly_id: room_fid.into(),
                    name: room_fid.into(),
                    description: String::new(),
                    area,
                },
                exits,
            ))
            .id()
    }

    #[test]
    fn move_updates_character_last_room_to_destination_friendly_ids() {
        use grim_engine_types::components::Character;
        use uuid::Uuid;
        let mut app = test_app();
        let room2 = spawn_room(&mut app, "town", "market", Exits::default());
        let mut exits = Exits::default();
        exits.exits.insert(Cardinal::North, room2);
        let room1 = spawn_room(&mut app, "town", "square", exits);
        let actor = app
            .world_mut()
            .spawn((
                InRoom { room: room1 },
                Character {
                    id: Uuid::new_v4(),
                    name: "Walker".into(),
                    account_id: Uuid::new_v4(),
                    created_at: chrono::Utc::now(),
                    last_room: None,
                    roles: Vec::new(),
                },
            ))
            .id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Move {
                direction: Cardinal::North,
            },
        });
        app.update();
        let loc = app
            .world()
            .get::<Character>(actor)
            .unwrap()
            .last_room
            .clone()
            .expect("last_room should be set after moving");
        assert_eq!(loc.area, "town");
        assert_eq!(loc.room, "market");
    }

    #[test]
    fn move_without_character_component_does_not_panic() {
        // A non-character actor (e.g. an NPC) can move; the last_room update is
        // simply skipped rather than erroring.
        let mut app = test_app();
        let room2 = spawn_room(&mut app, "town", "market", Exits::default());
        let mut exits = Exits::default();
        exits.exits.insert(Cardinal::North, room2);
        let room1 = spawn_room(&mut app, "town", "square", exits);
        let actor = app.world_mut().spawn(InRoom { room: room1 }).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Move {
                direction: Cardinal::North,
            },
        });
        app.update();
        assert_eq!(app.world().get::<InRoom>(actor).unwrap().room, room2);
    }
}
