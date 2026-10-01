//! Container verbs: `look in <container>`, `get <item> <container>`
//! (via `GetFrom`), `put <item> <container>` (via `PutIn`).
//!
//! Containers hold objects as `CarriedBy { carrier: <container> }`. Corpses
//! are one-way (`OneWay`): takes work, puts refuse. Ranking mirrors `get`
//! (exact name, exact keyword, shortest-prefix name; ties to lowest entity).

use bevy::prelude::*;
use grim_actor::placement::InRoom;
use grim_core::components::{Keywords, Name as GrimName};
use grim_core::events::{Command, EngineCommand, InfoMessage, ItemEvent, ItemKind};
use grim_target::{parse_target, query, ParseOptions};
use grim_text::tr;

use crate::boxes::{Boxes, BoxesOneWay};
use crate::object::{CarriedBy, Object};

/// Find a container in the actor's room: ground objects carrying
/// [`Container`], matched by ITEM spec.
fn find_container(spec: &grim_target::TargetSpec, room: Entity, objects: &Boxes) -> Option<Entity> {
    let found = query(
        spec,
        objects.iter().filter(|(_, _, _, ir)| ir.room == room).map(
            |(entity, name, keywords, _)| {
                (
                    entity,
                    name.0.as_str(),
                    keywords.map(|k| k.0.as_slice()).unwrap_or(&[]),
                )
            },
        ),
    );
    found.into_iter().next()
}

/// `look in <container>`: list the objects it holds, one per line.
pub(crate) fn handle_look_in(
    mut engine: MessageReader<EngineCommand>,
    inroom: Query<&InRoom>,
    objects: Boxes,
    carried: Query<(Entity, &GrimName, &CarriedBy), With<Object>>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::LookIn { container } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Ok(actor_room) = inroom.get(actor) else {
            continue;
        };
        let Some(spec) = parse_target(container, ParseOptions::ITEM) else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("container.get.not_found"),
            });
            continue;
        };
        let Some(entity) = find_container(&spec, actor_room.room, &objects) else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("container.get.not_found"),
            });
            continue;
        };
        let items: Vec<String> = carried
            .iter()
            .filter(|(_, _, held)| held.carrier == entity)
            .map(|(_, name, _)| {
                let short = name.0.clone();
                tr!("container.look.item", short = short.as_str())
            })
            .collect();
        let text = if items.is_empty() {
            tr!("container.look.empty")
        } else {
            items.concat()
        };
        info.write(InfoMessage {
            target: actor,
            text,
        });
    }
}

/// `get <item> <container>` (`GetFrom`): take matches out of a container
/// into the actor's pack. One [`ItemEvent`] per object, like ground `get`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_get_from(
    mut engine: MessageReader<EngineCommand>,
    mut commands: Commands,
    inroom: Query<&InRoom>,
    objects: Boxes,
    carried: Query<(Entity, &GrimName, Option<&Keywords>, &CarriedBy), With<Object>>,
    names: Query<&GrimName>,
    mut items: MessageWriter<ItemEvent>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::GetFrom { item, container } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let (Some(actor_room), Some(actor_name)) = (inroom.get(actor).ok(), names.get(actor).ok())
        else {
            continue;
        };
        let (Some(cspec), Some(ispec)) = (
            parse_target(container, ParseOptions::ITEM),
            parse_target(item, ParseOptions::ITEM),
        ) else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("container.get.not_found"),
            });
            continue;
        };
        let Some(holder) = find_container(&cspec, actor_room.room, &objects) else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("container.get.not_found"),
            });
            continue;
        };
        let found = query(
            &ispec,
            carried
                .iter()
                .filter(|(_, _, _, held)| held.carrier == holder)
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
                text: tr!("container.get.not_found"),
            });
            continue;
        }
        for entity in found {
            let Ok((_, name, _, _)) = carried.get(entity) else {
                continue;
            };
            let short = name.0.clone();
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

/// `put <item> <container>` (`PutIn`): place carried matches into a
/// container. One-way containers (corpses) refuse with a direct reply.
pub(crate) fn handle_put_in(
    mut engine: MessageReader<EngineCommand>,
    mut commands: Commands,
    inroom: Query<&InRoom>,
    objects: BoxesOneWay,
    carried: Query<(Entity, &GrimName, Option<&Keywords>, &CarriedBy), With<Object>>,
    names: Query<&GrimName>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::PutIn { item, container } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let (Some(actor_room), Some(actor_name)) = (inroom.get(actor).ok(), names.get(actor).ok())
        else {
            continue;
        };
        let _ = actor_name;
        let (Some(cspec), Some(ispec)) = (
            parse_target(container, ParseOptions::ITEM),
            parse_target(item, ParseOptions::ITEM),
        ) else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("container.get.not_found"),
            });
            continue;
        };
        let found_holder = query(
            &cspec,
            objects
                .iter()
                .filter(|(_, _, _, ir, _)| ir.room == actor_room.room)
                .map(|(entity, name, keywords, _, _)| {
                    (
                        entity,
                        name.0.as_str(),
                        keywords.map(|k| k.0.as_slice()).unwrap_or(&[]),
                    )
                }),
        );
        let Some(holder) = found_holder.into_iter().next() else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("container.get.not_found"),
            });
            continue;
        };
        if objects
            .get(holder)
            .is_ok_and(|(_, _, _, _, one_way)| one_way.is_some())
        {
            info.write(InfoMessage {
                target: actor,
                text: tr!("container.put.refused"),
            });
            continue;
        }
        let found = query(
            &ispec,
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
        let holder_name = objects
            .get(holder)
            .map(|(_, n, _, _, _)| n.0.clone())
            .unwrap_or_default();
        for entity in found {
            let Ok((_, name, _, _)) = carried.get(entity) else {
                continue;
            };
            let short = name.0.clone();
            commands
                .entity(entity)
                .insert(CarriedBy { carrier: holder });
            info.write(InfoMessage {
                target: actor,
                text: tr!(
                    "container.put.done",
                    short = short.as_str(),
                    container = holder_name.as_str()
                ),
            });
        }
    }
}

/// Wire the container verbs and the messages they read.
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_systems(Update, (handle_look_in, handle_get_from, handle_put_in));
}
