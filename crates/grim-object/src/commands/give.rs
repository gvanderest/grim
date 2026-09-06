//! `give <item> <target>`: hand a carried object to a being in the room.
//!
//! Player characters accept; creatures refuse ("They don't want that item.").
//! The move emits one [`TransferEvent`], which the scene layer renders
//! per-recipient (first-party to the giver, second-party to the recipient,
//! third-party to the rest of the room). Misses answer the giver directly
//! with an [`InfoMessage`].

use bevy::prelude::*;
use grim_actor::commands::look::rank_target;
use grim_actor::placement::InRoom;
use grim_actor::{Character, Creature, Linkdead, Player};
use grim_core::components::{Keywords, Name as GrimName};
use grim_core::events::{Command, EngineCommand, InfoMessage, TransferEvent, TransferKind};
use grim_text::tr;

use crate::object::CarriedBy;
use crate::persist::Carried;

/// Beings that can hold or receive objects: PCs (`Character`, online or
/// linkdead — mirroring `tell`'s `LivePc`) and creatures (`Creature`).
/// Half-built `Character`-only entities match neither marker and are skipped.
pub(crate) type Beings<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static InRoom,
        &'static GrimName,
        Option<&'static Keywords>,
        Option<&'static Character>,
        Option<&'static Creature>,
        Option<&'static Player>,
        Option<&'static Linkdead>,
    ),
>;

/// The best being in `room` matching `want` (never `actor`): exact name,
/// exact keyword, then shortest-prefix name, ties to the lowest entity id.
/// Returns the entity, its display name, and whether it is a creature.
/// Self-dealing is the caller's job (`is_self`); this only excludes the actor.
pub(crate) fn find_being(
    want: &str,
    room: Entity,
    actor: Entity,
    beings: &Beings,
) -> Option<(Entity, String, bool)> {
    if want.is_empty() {
        return None;
    }
    beings
        .iter()
        .filter(|(e, ir, _, _, ch, cr, p, l)| {
            *e != actor
                && ir.room == room
                && (cr.is_some() || (ch.is_some() && (p.is_some() || l.is_some())))
        })
        .filter_map(|(e, _, nm, kw, _, cr, _, _)| {
            rank_target(&nm.0, kw, want)
                .map(|rank| (rank, e.to_bits(), e, nm.0.clone(), cr.is_some()))
        })
        .min()
        .map(|(_, _, e, name, is_creature)| (e, name, is_creature))
}

/// `give <item> <target>`: move the best-matching carried object into a being's
/// pack. PCs accept; creatures refuse.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_give(
    mut engine: MessageReader<EngineCommand>,
    mut commands: Commands,
    inroom: Query<&InRoom>,
    carried: Carried,
    beings: Beings,
    names: Query<&GrimName>,
    mut transfers: MessageWriter<TransferEvent>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Give { item, target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Ok(actor_room) = inroom.get(actor) else {
            continue;
        };
        let Ok(actor_name) = names.get(actor) else {
            continue;
        };
        // Your own hands first: `self` (or your own name) can never receive.
        if is_self(target, &actor_name.0) {
            info.write(InfoMessage {
                target: actor,
                text: tr!("item.give.self"),
            });
            continue;
        }
        let want_item = item.to_lowercase();
        let held = carried
            .iter()
            .filter(|(_, _, _, _, _, held)| held.carrier == actor)
            .filter_map(|(e, nm, _, kw, _, _)| {
                rank_target(&nm.0, kw, &want_item).map(|rank| (rank, e.to_bits(), e, nm.0.clone()))
            })
            .min();
        let Some((_, _, entity, short)) = held else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("item.give.not_carried"),
            });
            continue;
        };
        match find_being(&target.to_lowercase(), actor_room.room, actor, &beings) {
            Some((recipient, recipient_name, false)) => {
                commands
                    .entity(entity)
                    .insert(CarriedBy { carrier: recipient });
                transfers.write(TransferEvent {
                    mover: actor,
                    mover_name: actor_name.0.clone(),
                    other: recipient,
                    other_name: recipient_name,
                    room: actor_room.room,
                    short,
                    kind: TransferKind::Give,
                });
            }
            Some((_, _, true)) => {
                info.write(InfoMessage {
                    target: actor,
                    text: tr!("item.give.refused"),
                });
            }
            None => {
                info.write(InfoMessage {
                    target: actor,
                    text: tr!("item.give.not_here"),
                });
            }
        }
    }
}

/// `self`, or the actor's own exact name, as a transfer peer.
pub(crate) fn is_self(target: &str, actor_name: &str) -> bool {
    let want = target.trim().to_lowercase();
    want == "self" || want == actor_name.to_lowercase()
}

/// Wire the `give` handler and the messages it reads. [`TransferEvent`] is
/// registered by the plugin (shared with `steal`).
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_systems(Update, handle_give);
}

#[cfg(test)]
mod tests {
    use super::*;
    use grim_actor::{Character, Creature, Player};
    use grim_core::components::Keywords;
    use grim_core::GrimId;

    use crate::object::Object;

    fn character() -> Character {
        Character {
            id: GrimId::new(),
            account_id: GrimId::new(),
            created_at: chrono::Utc::now(),
            last_room: None,
            roles: Vec::new(),
            class: String::new(),
            title: None,
            restrings: std::collections::HashMap::new(),
        }
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<TransferEvent>();
        register(&mut app);
        app
    }

    fn room_with_giver(app: &mut App) -> (Entity, Entity) {
        let room = app.world_mut().spawn_empty().id();
        let giver = app
            .world_mut()
            .spawn((
                GrimName("Alice".into()),
                character(),
                Player {
                    connection: Entity::PLACEHOLDER,
                },
                InRoom { room },
            ))
            .id();
        (room, giver)
    }

    fn spawn_pc(app: &mut App, room: Entity, name: &str) -> Entity {
        app.world_mut()
            .spawn((
                GrimName(name.into()),
                character(),
                Player {
                    connection: Entity::PLACEHOLDER,
                },
                InRoom { room },
            ))
            .id()
    }

    fn spawn_mob(app: &mut App, room: Entity, name: &str) -> Entity {
        app.world_mut()
            .spawn((GrimName(name.into()), Creature, InRoom { room }))
            .id()
    }
    fn give_item(app: &mut App, holder: Entity, name: &str, keywords: &[&str]) -> Entity {
        app.world_mut()
            .spawn((
                Object,
                GrimName(name.into()),
                Keywords(keywords.iter().map(|k| k.to_string()).collect()),
                CarriedBy { carrier: holder },
            ))
            .id()
    }

    fn transfers(app: &App) -> Vec<TransferEvent> {
        let messages = app.world().resource::<Messages<TransferEvent>>();
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

    fn give(app: &mut App, giver: Entity, item: &str, target: &str) {
        app.world_mut().write_message(EngineCommand {
            client: giver,
            command: Command::Give {
                item: item.into(),
                target: target.into(),
            },
        });
        app.update();
    }

    #[test]
    fn give_moves_item_and_emits_event() {
        let mut app = test_app();
        let (room, giver) = room_with_giver(&mut app);
        let recipient = spawn_pc(&mut app, room, "Bob");
        let obj = give_item(&mut app, giver, "brass lantern", &["lantern"]);
        give(&mut app, giver, "lantern", "bob");

        assert_eq!(
            app.world().get::<CarriedBy>(obj).unwrap().carrier,
            recipient
        );
        let evs = transfers(&app);
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, TransferKind::Give);
        assert_eq!(evs[0].mover, giver);
        assert_eq!(evs[0].mover_name, "Alice");
        assert_eq!(evs[0].other, recipient);
        assert_eq!(evs[0].other_name, "Bob");
        assert_eq!(evs[0].short, "brass lantern");
        assert!(infos(&app).is_empty());
    }

    #[test]
    fn give_to_creature_is_refused_and_kept() {
        let mut app = test_app();
        let (room, giver) = room_with_giver(&mut app);
        spawn_mob(&mut app, room, "Goblin");
        let obj = give_item(&mut app, giver, "coin", &["coin"]);
        give(&mut app, giver, "coin", "goblin");

        assert_eq!(app.world().get::<CarriedBy>(obj).unwrap().carrier, giver);
        assert!(transfers(&app).is_empty());
        assert_eq!(
            infos(&app),
            vec![(giver, "They don't want that item.\n".to_string())]
        );
    }

    #[test]
    fn give_misses_unheld_item() {
        let mut app = test_app();
        let (room, giver) = room_with_giver(&mut app);
        spawn_pc(&mut app, room, "Bob");
        give(&mut app, giver, "lantern", "bob");

        assert!(transfers(&app).is_empty());
        assert_eq!(
            infos(&app),
            vec![(giver, "You aren't carrying that.\n".to_string())]
        );
    }

    #[test]
    fn give_misses_absent_target() {
        let mut app = test_app();
        let (_room, giver) = room_with_giver(&mut app);
        give_item(&mut app, giver, "coin", &["coin"]);
        give(&mut app, giver, "coin", "nobody");

        assert!(transfers(&app).is_empty());
        assert_eq!(
            infos(&app),
            vec![(giver, "They aren't here.\n".to_string())]
        );
    }

    #[test]
    fn give_to_other_room_is_not_here() {
        let mut app = test_app();
        let (_room, giver) = room_with_giver(&mut app);
        let elsewhere = app.world_mut().spawn_empty().id();
        spawn_pc(&mut app, elsewhere, "Bob");
        give_item(&mut app, giver, "coin", &["coin"]);
        give(&mut app, giver, "coin", "bob");

        assert!(transfers(&app).is_empty());
        assert_eq!(infos(&app).len(), 1);
    }

    #[test]
    fn give_to_self_is_refused() {
        let mut app = test_app();
        let (_room, giver) = room_with_giver(&mut app);
        give_item(&mut app, giver, "coin", &["coin"]);
        give(&mut app, giver, "coin", "self");

        assert!(transfers(&app).is_empty());
        assert_eq!(
            infos(&app),
            vec![(giver, "You can't give something to yourself.\n".to_string())]
        );
    }
}
