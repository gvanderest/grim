//! `get` / `drop`: move objects between a room and a carrier's hands.
//!
//! Targets are [`grim_target`] specs ([`ParseOptions::ITEM`]): one match by
//! default, `2.coin` the second, `3*coin` up to three, `all [words]` every
//! match. Each moved object emits one [`ItemEvent`] fact, which the scene
//! layer renders per-recipient (first-party "You …" to the actor,
//! third-party "<name> …" to the rest of the room). Misses answer the actor
//! directly with an [`InfoMessage`].

use bevy::prelude::*;
use grim_actor::placement::InRoom;
use grim_core::components::{Keywords, Name as GrimName};
use grim_core::events::{Command, EngineCommand, InfoMessage, ItemEvent, ItemKind};
use grim_target::{parse_target, query, ParseOptions};
use grim_text::tr;

use crate::object::{CarriedBy, Object};

/// Ground objects: marker, room placement, no carrier. Carried objects match
/// neither bound (`InRoom` is required), so they are invisible to `get` even
/// when the holder stands in the room.
type Ground<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static InRoom,
        &'static GrimName,
        Option<&'static Keywords>,
    ),
    (With<Object>, Without<CarriedBy>),
>;

/// `get <target>`: pick up the matching objects in the actor's room.
/// Ranking mirrors `look` (exact name, exact keyword, shortest-prefix name);
/// ties fall to the lowest entity id, keeping resolution deterministic.
pub(crate) fn handle_get(
    mut engine: MessageReader<EngineCommand>,
    mut commands: Commands,
    inroom: Query<&InRoom>,
    objects: Ground,
    names: Query<&GrimName>,
    mut items: MessageWriter<ItemEvent>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Get { target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Ok(actor_room) = inroom.get(actor) else {
            continue;
        };
        let Ok(actor_name) = names.get(actor) else {
            continue;
        };
        let Some(spec) = parse_target(target, ParseOptions::ITEM) else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("item.get.not_found"),
            });
            continue;
        };
        let found = query(
            &spec,
            objects
                .iter()
                .filter(|(_, ir, _, _)| ir.room == actor_room.room)
                .map(|(entity, _, name, keywords)| {
                    (
                        entity,
                        name.0.as_str(),
                        keywords.map(|k| k.0.as_slice()).unwrap_or(&[]),
                    )
                }),
        );
        if found.is_empty() {
            info.write(InfoMessage {
                target: actor,
                text: tr!("item.get.not_found"),
            });
            continue;
        }
        for entity in found {
            let Ok((_, _, name, _)) = objects.get(entity) else {
                continue;
            };
            let short = name.0.clone();
            commands.entity(entity).remove::<InRoom>();
            commands.entity(entity).insert(CarriedBy { carrier: actor });
            items.write(ItemEvent {
                actor,
                room: actor_room.room,
                actor_name: actor_name.0.clone(),
                short,
                kind: ItemKind::Pickup,
            });
        }
    }
}

/// `drop <target>`: drop the matching carried objects into the actor's room.
/// Matching runs over the carrier's objects only, with the same ranking as
/// `get`.
pub(crate) fn handle_drop(
    mut engine: MessageReader<EngineCommand>,
    mut commands: Commands,
    inroom: Query<&InRoom>,
    carried: Query<(Entity, &GrimName, Option<&Keywords>, &CarriedBy), With<Object>>,
    names: Query<&GrimName>,
    mut items: MessageWriter<ItemEvent>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Drop { target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Ok(actor_room) = inroom.get(actor) else {
            continue;
        };
        let Ok(actor_name) = names.get(actor) else {
            continue;
        };
        let Some(spec) = parse_target(target, ParseOptions::ITEM) else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("item.drop.not_carried"),
            });
            continue;
        };
        let found = query(
            &spec,
            carried
                .iter()
                .filter(|(_, _, _, held)| held.carrier == actor)
                .map(|(entity, name, keywords, _)| {
                    (
                        entity,
                        name.0.as_str(),
                        keywords.map(|k| k.0.as_slice()).unwrap_or(&[]),
                    )
                }),
        );
        if found.is_empty() {
            info.write(InfoMessage {
                target: actor,
                text: tr!("item.drop.not_carried"),
            });
            continue;
        }
        for entity in found {
            let Ok((_, name, _, _)) = carried.get(entity) else {
                continue;
            };
            let short = name.0.clone();
            commands.entity(entity).remove::<CarriedBy>();
            commands.entity(entity).insert(InRoom {
                room: actor_room.room,
            });
            items.write(ItemEvent {
                actor,
                room: actor_room.room,
                actor_name: actor_name.0.clone(),
                short,
                kind: ItemKind::Drop,
            });
        }
    }
}

/// Wire the get/drop handlers and the messages they read. [`ItemEvent`] is
/// registered by the plugin (shared by every verb that emits it).
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_systems(Update, (handle_get, handle_drop));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        // `ItemEvent` lives in grim-core (transitional game-event home);
        // the plugin registers it, the bare handler registration does not.
        app.add_message::<ItemEvent>();
        register(&mut app);
        app
    }

    fn room_with_actor(app: &mut App) -> (Entity, Entity) {
        let room = app.world_mut().spawn_empty().id();
        let actor = app
            .world_mut()
            .spawn((GrimName("Alice".into()), InRoom { room }))
            .id();
        (room, actor)
    }

    fn spawn_object(app: &mut App, room: Entity, name: &str, keywords: &[&str]) -> Entity {
        app.world_mut()
            .spawn((
                Object,
                GrimName(name.into()),
                Keywords(keywords.iter().map(|k| k.to_string()).collect()),
                InRoom { room },
            ))
            .id()
    }

    fn item_events(app: &App) -> Vec<ItemEvent> {
        let messages = app.world().resource::<Messages<ItemEvent>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).cloned().collect()
    }

    fn infos(app: &App) -> Vec<(Entity, String)> {
        let messages = app.world().resource::<Messages<InfoMessage>>();
        let mut cursor = messages.get_cursor();
        cursor
            .read(messages)
            .map(|m| (m.target, m.text.clone()))
            .collect()
    }

    #[test]
    fn get_picks_up_by_keyword_and_emits_pickup() {
        let mut app = test_app();
        let (room, actor) = room_with_actor(&mut app);
        let obj = spawn_object(&mut app, room, "brass lantern", &["lantern", "brass"]);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Get {
                target: "lantern".into(),
            },
        });
        app.update();
        // Placement swapped: no room, one carrier.
        assert!(app.world().get::<InRoom>(obj).is_none());
        assert_eq!(app.world().get::<CarriedBy>(obj).unwrap().carrier, actor);
        // One pickup fact with the actor's name and the object's short.
        let evs = item_events(&app);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, ItemKind::Pickup);
        assert_eq!(evs[0].actor, actor);
        assert_eq!(evs[0].room, room);
        assert_eq!(evs[0].actor_name, "Alice");
        assert_eq!(evs[0].short, "brass lantern");
        assert!(infos(&app).is_empty());
    }

    #[test]
    fn get_ranks_exact_name_over_keyword_prefix() {
        let mut app = test_app();
        let (room, actor) = room_with_actor(&mut app);
        let pole = spawn_object(&mut app, room, "lantern pole", &["pole"]);
        let exact = spawn_object(&mut app, room, "lantern", &["lamp"]);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Get {
                target: "lantern".into(),
            },
        });
        app.update();
        let evs = item_events(&app);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].short, "lantern");
        assert!(app.world().get::<InRoom>(exact).is_none());
        assert!(app.world().get::<InRoom>(pole).is_some());
    }

    #[test]
    fn get_matches_name_prefix() {
        let mut app = test_app();
        let (room, actor) = room_with_actor(&mut app);
        spawn_object(&mut app, room, "brass lantern", &["lantern"]);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Get {
                target: "bras".into(),
            },
        });
        app.update();
        let evs = item_events(&app);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].short, "brass lantern");
    }

    #[test]
    fn get_ignores_other_rooms() {
        let mut app = test_app();
        let (_room, actor) = room_with_actor(&mut app);
        let elsewhere = app.world_mut().spawn_empty().id();
        spawn_object(&mut app, elsewhere, "coin", &["coin"]);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Get {
                target: "coin".into(),
            },
        });
        app.update();
        assert!(item_events(&app).is_empty());
        assert_eq!(infos(&app).len(), 1);
    }

    #[test]
    fn get_miss_answers_not_found() {
        let mut app = test_app();
        let (_room, actor) = room_with_actor(&mut app);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Get {
                target: "sword".into(),
            },
        });
        app.update();
        assert!(item_events(&app).is_empty());
        assert_eq!(
            infos(&app),
            vec![(actor, "You don't see that here.\n".to_string())]
        );
    }

    #[test]
    fn get_ignores_carried_objects() {
        let mut app = test_app();
        let (room, actor) = room_with_actor(&mut app);
        let obj = spawn_object(&mut app, room, "coin", &["coin"]);
        // Already carried (no InRoom): invisible to get.
        app.world_mut().entity_mut(obj).remove::<InRoom>();
        app.world_mut()
            .entity_mut(obj)
            .insert(CarriedBy { carrier: actor });
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Get {
                target: "coin".into(),
            },
        });
        app.update();
        assert!(item_events(&app).is_empty());
        assert_eq!(infos(&app).len(), 1);
    }

    #[test]
    fn drop_returns_object_to_room_and_emits_drop() {
        let mut app = test_app();
        let (room, actor) = room_with_actor(&mut app);
        let obj = spawn_object(&mut app, room, "brass lantern", &["lantern"]);
        app.world_mut().entity_mut(obj).remove::<InRoom>();
        app.world_mut()
            .entity_mut(obj)
            .insert(CarriedBy { carrier: actor });
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Drop {
                target: "lantern".into(),
            },
        });
        app.update();

        assert!(app.world().get::<CarriedBy>(obj).is_none());
        assert_eq!(app.world().get::<InRoom>(obj).unwrap().room, room);
        let evs = item_events(&app);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, ItemKind::Drop);
        assert_eq!(evs[0].short, "brass lantern");
        assert!(infos(&app).is_empty());
    }

    #[test]
    fn drop_miss_answers_not_carried() {
        let mut app = test_app();
        let (room, actor) = room_with_actor(&mut app);
        // On the ground is not carried.
        spawn_object(&mut app, room, "coin", &["coin"]);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Drop {
                target: "coin".into(),
            },
        });
        app.update();
        assert!(item_events(&app).is_empty());
        assert_eq!(
            infos(&app),
            vec![(actor, "You aren't carrying that.\n".to_string())]
        );
    }

    #[test]
    fn drop_only_considers_the_actors_objects() {
        let mut app = test_app();
        let (room, alice) = room_with_actor(&mut app);
        let bob = app
            .world_mut()
            .spawn((GrimName("Bob".into()), InRoom { room }))
            .id();
        let bobs = spawn_object(&mut app, room, "coin", &["coin"]);
        app.world_mut().entity_mut(bobs).remove::<InRoom>();
        app.world_mut()
            .entity_mut(bobs)
            .insert(CarriedBy { carrier: bob });
        app.world_mut().write_message(EngineCommand {
            client: alice,
            command: Command::Drop {
                target: "coin".into(),
            },
        });
        app.update();
        // Bob still carries it; Alice got the miss line.
        assert_eq!(app.world().get::<CarriedBy>(bobs).unwrap().carrier, bob);
        assert_eq!(infos(&app).len(), 1);
    }

    #[test]
    fn get_quantity_picks_up_to_count() {
        let mut app = test_app();
        let (_room, actor) = room_with_actor(&mut app);
        // Three coins on the ground; `2*coin` takes two.
        let room = app.world().get::<InRoom>(actor).unwrap().room;
        let (a, b, c) = (
            spawn_object(&mut app, room, "coin", &["coin"]),
            spawn_object(&mut app, room, "coin", &["coin"]),
            spawn_object(&mut app, room, "coin", &["coin"]),
        );
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Get {
                target: "2*coin".into(),
            },
        });
        app.update();
        let evs = item_events(&app);
        assert_eq!(evs.len(), 2);
        assert!(evs.iter().all(|ev| ev.kind == ItemKind::Pickup));
        let carried: Vec<_> = [a, b, c]
            .into_iter()
            .filter(|e| app.world().get::<CarriedBy>(*e).is_some())
            .collect();
        assert_eq!(carried.len(), 2);
        assert!(infos(&app).is_empty());
    }

    #[test]
    fn get_all_picks_up_every_match() {
        let mut app = test_app();
        let (room, actor) = room_with_actor(&mut app);
        let coin = spawn_object(&mut app, room, "coin", &["coin"]);
        let sword = spawn_object(&mut app, room, "sword", &["sword"]);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Get {
                target: "all".into(),
            },
        });
        app.update();
        assert_eq!(item_events(&app).len(), 2);
        for obj in [coin, sword] {
            assert_eq!(app.world().get::<CarriedBy>(obj).unwrap().carrier, actor);
        }
        assert!(infos(&app).is_empty());
    }

    #[test]
    fn get_offset_picks_second_ranked() {
        let mut app = test_app();
        let (room, actor) = room_with_actor(&mut app);
        // Exact ("coin") outranks prefix ("coins"), so `2.coin` is "coins".
        let exact = spawn_object(&mut app, room, "coin", &["coin"]);
        let prefixed = spawn_object(&mut app, room, "coins", &["coins"]);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Get {
                target: "2.coin".into(),
            },
        });
        app.update();
        let evs = item_events(&app);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].short, "coins");
        assert!(app.world().get::<InRoom>(prefixed).is_none());
        assert!(app.world().get::<InRoom>(exact).is_some());
    }

    #[test]
    fn get_invalid_spec_answers_not_found() {
        let mut app = test_app();
        let (room, actor) = room_with_actor(&mut app);
        let obj = spawn_object(&mut app, room, "coin", &["coin"]);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Get {
                target: "2.".into(),
            },
        });
        app.update();
        assert!(item_events(&app).is_empty());
        assert!(app.world().get::<InRoom>(obj).is_some());
        assert_eq!(
            infos(&app),
            vec![(actor, "You don't see that here.\n".to_string())]
        );
    }

    #[test]
    fn get_quoted_group_matches_every_term() {
        let mut app = test_app();
        let (room, actor) = room_with_actor(&mut app);
        // No keywords: only the multi-term AND can match the inner word.
        let obj = spawn_object(&mut app, room, "brass lantern", &[]);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Get {
                target: "\"brass lantern\"".into(),
            },
        });
        app.update();
        let evs = item_events(&app);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].short, "brass lantern");
        assert!(app.world().get::<InRoom>(obj).is_none());
    }

    #[test]
    fn drop_all_drops_everything_carried() {
        let mut app = test_app();
        let (room, actor) = room_with_actor(&mut app);
        let (a, b) = (
            spawn_object(&mut app, room, "coin", &["coin"]),
            spawn_object(&mut app, room, "sword", &["sword"]),
        );
        for obj in [a, b] {
            app.world_mut().entity_mut(obj).remove::<InRoom>();
            app.world_mut()
                .entity_mut(obj)
                .insert(CarriedBy { carrier: actor });
        }
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Drop {
                target: "all".into(),
            },
        });
        app.update();
        let evs = item_events(&app);
        assert_eq!(evs.len(), 2);
        assert!(evs.iter().all(|ev| ev.kind == ItemKind::Drop));
        for obj in [a, b] {
            assert!(app.world().get::<CarriedBy>(obj).is_none());
            assert_eq!(app.world().get::<InRoom>(obj).unwrap().room, room);
        }
        assert!(infos(&app).is_empty());
    }

    #[test]
    fn drop_offset_drops_second_ranked() {
        let mut app = test_app();
        let (room, actor) = room_with_actor(&mut app);
        let exact = spawn_object(&mut app, room, "coin", &["coin"]);
        let prefixed = spawn_object(&mut app, room, "coins", &["coins"]);
        for obj in [exact, prefixed] {
            app.world_mut().entity_mut(obj).remove::<InRoom>();
            app.world_mut()
                .entity_mut(obj)
                .insert(CarriedBy { carrier: actor });
        }
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Drop {
                target: "2.coin".into(),
            },
        });
        app.update();
        let evs = item_events(&app);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].short, "coins");
        assert_eq!(app.world().get::<InRoom>(prefixed).unwrap().room, room);
        assert!(app.world().get::<CarriedBy>(exact).is_some());
    }
}
