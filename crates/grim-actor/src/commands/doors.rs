//! `open` / `close`: flip the door guarding an exit, on both sides.
//!
//! Both linked rooms carry their own copy of the door (see
//! `grim_world::Doors`); the handler flips the pair. When the reverse exit
//! does not point back, only the actor's side flips — a one-sided door, not
//! a failure.
//!
//! Reads the actor's [`Character`]/[`InRoom`] plus `grim_world`'s room
//! topology (`Exits`/`Doors`), emits [`DoorEvent`] for per-recipient
//! rendering in `grim-scene`.

use bevy::prelude::*;
use grim_core::events::{Command, DoorEvent, EngineCommand, InfoMessage};
use grim_text::tr;
use grim_world::{Doors, Exits};

use crate::placement::InRoom;

/// `open <direction>`: open the door on that exit, if any.
pub(crate) fn handle_open(
    mut engine: MessageReader<EngineCommand>,
    mut info: MessageWriter<InfoMessage>,
    mut doors_events: MessageWriter<DoorEvent>,
    inroom: Query<&InRoom>,
    exits: Query<&Exits>,
    mut doors: Query<&mut Doors>,
) {
    for cmd in engine.read() {
        let Command::Open { direction } = cmd.command else {
            continue;
        };
        set_door(
            cmd.client,
            direction,
            true,
            &mut info,
            &mut doors_events,
            &inroom,
            &exits,
            &mut doors,
        );
    }
}

/// `close <direction>`: close the door on that exit, if any.
pub(crate) fn handle_close(
    mut engine: MessageReader<EngineCommand>,
    mut info: MessageWriter<InfoMessage>,
    mut doors_events: MessageWriter<DoorEvent>,
    inroom: Query<&InRoom>,
    exits: Query<&Exits>,
    mut doors: Query<&mut Doors>,
) {
    for cmd in engine.read() {
        let Command::Close { direction } = cmd.command else {
            continue;
        };
        set_door(
            cmd.client,
            direction,
            false,
            &mut info,
            &mut doors_events,
            &inroom,
            &exits,
            &mut doors,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn set_door(
    actor: Entity,
    direction: grim_core::cardinal::Cardinal,
    open: bool,
    info: &mut MessageWriter<InfoMessage>,
    doors_events: &mut MessageWriter<DoorEvent>,
    inroom: &Query<&InRoom>,
    exits: &Query<&Exits>,
    doors: &mut Query<&mut Doors>,
) {
    let from = match inroom.get(actor) {
        Ok(ir) => ir.room,
        Err(_) => return,
    };
    // Fail closed: no exit here means no door here, never free passage.
    let to = match exits
        .get(from)
        .ok()
        .and_then(|e| e.exits.get(&direction).copied())
    {
        Some(to) => to,
        None => {
            info.write(InfoMessage {
                target: actor,
                text: tr!("door.error.no_door"),
            });
            return;
        }
    };
    let name = match doors.get(from).ok().and_then(|d| d.doors.get(&direction)) {
        Some(door) => (door.name.clone(), door.open),
        None => {
            info.write(InfoMessage {
                target: actor,
                text: tr!("door.error.no_door"),
            });
            return;
        }
    };
    if name.1 == open {
        info.write(InfoMessage {
            target: actor,
            text: if open {
                tr!("door.open.already", name = name.0)
            } else {
                tr!("door.close.already", name = name.0)
            },
        });
        return;
    }
    if let Ok(mut own) = doors.get_mut(from) {
        if let Some(door) = own.doors.get_mut(&direction) {
            door.open = open;
        }
    }
    // Mirror the far side: the reverse exit's door flips too, when the link
    // points back at us.
    let back = direction.opposite();
    let reverse_ok = exits.get(to).ok().and_then(|e| e.exits.get(&back).copied()) == Some(from);
    if reverse_ok {
        if let Ok(mut far) = doors.get_mut(to) {
            if let Some(door) = far.doors.get_mut(&back) {
                door.open = open;
            }
        }
    }
    doors_events.write(DoorEvent {
        actor,
        from,
        to,
        direction,
        opened: open,
        name: name.0,
    });
}

/// Wire the `open`/`close` handlers and the delivery messages they own. The
/// `DoorEvent` facts they emit are registered by `grim_world::WorldPlugin`.
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_systems(Update, (handle_open, handle_close));
}

#[cfg(test)]
mod tests {
    use super::*;
    use grim_core::cardinal::Cardinal;
    use grim_core::character::Gender;
    use grim_core::components::Name as GrimName;
    use grim_core::GrimId;
    use grim_world::Area;
    use std::collections::HashMap;

    use crate::actor::Actor;
    use crate::character::Role;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(grim_world::WorldPlugin);
        register(&mut app);
        app
    }

    fn spawn_room(app: &mut App, exits: Exits, doors: Doors) -> Entity {
        let area = app
            .world_mut()
            .spawn(Area {
                id: GrimId::new(),
                friendly_id: "haven".into(),
                name: "haven".into(),
            })
            .id();
        app.world_mut()
            .spawn((
                grim_world::Room {
                    id: GrimId::new(),
                    friendly_id: "room".into(),
                    name: "room".into(),
                    description: String::new(),
                    area,
                },
                exits,
                doors,
            ))
            .id()
    }

    fn spawn_actor_in(app: &mut App, room: Entity) -> Entity {
        app.world_mut()
            .spawn((
                InRoom { room },
                GrimName("Hero".into()),
                Actor {
                    race: String::new(),
                    level: 1,
                    gender: Gender::Neutral,
                },
                crate::character::Character {
                    id: GrimId::new(),
                    account_id: GrimId::new(),
                    created_at: chrono::Utc::now(),
                    last_room: None,
                    roles: vec![Role::Admin],
                    class: String::new(),
                    title: None,
                    restrings: HashMap::new(),
                    config: HashMap::new(),
                },
            ))
            .id()
    }

    fn link(app: &mut App, a: Entity, b: Entity) {
        app.world_mut()
            .get_mut::<Exits>(a)
            .unwrap()
            .exits
            .insert(Cardinal::East, b);
        app.world_mut()
            .get_mut::<Exits>(b)
            .unwrap()
            .exits
            .insert(Cardinal::West, a);
    }

    fn hang(app: &mut App, room: Entity, dir: Cardinal, open: bool) {
        app.world_mut()
            .get_mut::<Doors>(room)
            .unwrap()
            .doors
            .insert(
                dir,
                grim_world::Door {
                    name: "the privy door".into(),
                    keywords: vec!["privy door".into()],
                    open,
                },
            );
    }

    fn send(app: &mut App, actor: Entity, command: Command) {
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command,
        });
        app.update();
    }

    fn infos(app: &App) -> Vec<String> {
        let messages = app.world().resource::<Messages<InfoMessage>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|m| m.text.clone()).collect()
    }

    fn door_events(app: &App) -> Vec<(bool, String)> {
        let messages = app.world().resource::<Messages<DoorEvent>>();
        let mut cursor = messages.get_cursor();
        cursor
            .read(messages)
            .map(|e| (e.opened, e.name.clone()))
            .collect()
    }

    #[test]
    fn open_flips_both_sides_and_emits_fact() {
        let mut app = test_app();
        let a = spawn_room(&mut app, Exits::default(), Doors::default());
        let b = spawn_room(&mut app, Exits::default(), Doors::default());
        link(&mut app, a, b);
        hang(&mut app, a, Cardinal::East, false);
        hang(&mut app, b, Cardinal::West, false);
        let actor = spawn_actor_in(&mut app, a);
        send(
            &mut app,
            actor,
            Command::Open {
                direction: Cardinal::East,
            },
        );
        assert!(app.world().get::<Doors>(a).unwrap().doors[&Cardinal::East].open);
        assert!(app.world().get::<Doors>(b).unwrap().doors[&Cardinal::West].open);
        assert_eq!(
            door_events(&app),
            vec![(true, "the privy door".to_string())]
        );
        assert!(infos(&app).is_empty());
    }

    #[test]
    fn open_with_no_door_replies_no_door() {
        let mut app = test_app();
        let a = spawn_room(&mut app, Exits::default(), Doors::default());
        let b = spawn_room(&mut app, Exits::default(), Doors::default());
        link(&mut app, a, b);
        let actor = spawn_actor_in(&mut app, a);
        send(
            &mut app,
            actor,
            Command::Open {
                direction: Cardinal::East,
            },
        );
        assert_eq!(
            infos(&app),
            vec!["There is no door in that direction.\n".to_string()]
        );
        assert!(door_events(&app).is_empty());
    }

    #[test]
    fn open_with_no_exit_replies_no_door() {
        let mut app = test_app();
        let a = spawn_room(&mut app, Exits::default(), Doors::default());
        let actor = spawn_actor_in(&mut app, a);
        send(
            &mut app,
            actor,
            Command::Close {
                direction: Cardinal::North,
            },
        );
        assert_eq!(
            infos(&app),
            vec!["There is no door in that direction.\n".to_string()]
        );
    }

    #[test]
    fn reopen_replies_already_open() {
        let mut app = test_app();
        let a = spawn_room(&mut app, Exits::default(), Doors::default());
        let b = spawn_room(&mut app, Exits::default(), Doors::default());
        link(&mut app, a, b);
        hang(&mut app, a, Cardinal::East, true);
        hang(&mut app, b, Cardinal::West, true);
        let actor = spawn_actor_in(&mut app, a);
        send(
            &mut app,
            actor,
            Command::Open {
                direction: Cardinal::East,
            },
        );
        assert!(infos(&app).iter().any(|t| t.contains("already open")));
        assert!(door_events(&app).is_empty());
    }

    #[test]
    fn close_flips_and_reclose_replies_already_closed() {
        let mut app = test_app();
        let a = spawn_room(&mut app, Exits::default(), Doors::default());
        let b = spawn_room(&mut app, Exits::default(), Doors::default());
        link(&mut app, a, b);
        hang(&mut app, a, Cardinal::East, true);
        hang(&mut app, b, Cardinal::West, true);
        let actor = spawn_actor_in(&mut app, a);
        send(
            &mut app,
            actor,
            Command::Close {
                direction: Cardinal::East,
            },
        );
        assert!(!app.world().get::<Doors>(a).unwrap().doors[&Cardinal::East].open);
        assert!(!app.world().get::<Doors>(b).unwrap().doors[&Cardinal::West].open);
        assert_eq!(
            door_events(&app),
            vec![(false, "the privy door".to_string())]
        );
        send(
            &mut app,
            actor,
            Command::Close {
                direction: Cardinal::East,
            },
        );
        assert!(infos(&app).iter().any(|t| t.contains("already closed")));
    }

    #[test]
    fn one_sided_door_flips_own_side_only() {
        // Reverse exit missing: the actor's side still opens, no far mirror.
        let mut app = test_app();
        let a = spawn_room(&mut app, Exits::default(), Doors::default());
        let b = spawn_room(&mut app, Exits::default(), Doors::default());
        app.world_mut()
            .get_mut::<Exits>(a)
            .unwrap()
            .exits
            .insert(Cardinal::East, b);
        hang(&mut app, a, Cardinal::East, false);
        let actor = spawn_actor_in(&mut app, a);
        send(
            &mut app,
            actor,
            Command::Open {
                direction: Cardinal::East,
            },
        );
        assert!(app.world().get::<Doors>(a).unwrap().doors[&Cardinal::East].open);
        assert_eq!(door_events(&app).len(), 1);
    }
}
