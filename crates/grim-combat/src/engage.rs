//! Engagement: the room-scoped fight and the per-being target lists.
//!
//! A `Combat` sits on the room entity while at least one member fights there;
//! each member carries `Engaged { targets }` (from `grim-actor`). Attacking
//! appends both ways so victim and attacker see each other as targets.
//! Movement removes only the mover's own `Engaged`; [`prune`] owns the room
//! side (no cross-crate cycle) and despawns empty fights.

use bevy::prelude::*;
use grim_actor::{Engaged, Health, Posture};
use grim_core::components::Name as GrimName;
use grim_core::events::{Command, EngineCommand, InfoMessage};
use grim_target::{parse_target, ParseOptions};
use grim_text::tr;

use crate::corpse::is_corpse;
use crate::events::{DamageKind, FightStart};
use crate::formula::resolve_strike;
use crate::state::CombatRng;

/// A live fight in one room. Present on the room entity while non-empty.
#[derive(Component, Debug, Default)]
pub struct Combat {
    pub members: Vec<Entity>,
}

/// Ensure `a`↔`b` mutual engagement + room Combat membership: insert
/// `Engaged` where missing, append otherwise (index 0 never disturbed),
/// create-or-extend the room's `Combat`.
pub fn ensure_engaged(world: &mut World, a: Entity, b: Entity, room: Entity) {
    for (being, other) in [(a, b), (b, a)] {
        if world.get_entity(being).is_err() {
            continue;
        }
        if world.get::<Engaged>(being).is_none() {
            world.entity_mut(being).insert(Engaged {
                targets: vec![other],
            });
        } else if let Some(mut e) = world.get_mut::<Engaged>(being) {
            e.add(other);
        }
    }
    if world.get::<Combat>(room).is_none() {
        world.entity_mut(room).insert(Combat {
            members: vec![a, b],
        });
    } else if let Some(mut c) = world.get_mut::<Combat>(room) {
        for m in [a, b] {
            if !c.members.contains(&m) {
                c.members.push(m);
            }
        }
    }
}

/// Remove `being` from the fight entirely: drop its `Engaged`, pull it from
/// its room's [`Combat`], despawn the `Combat` when empty. Despawned beings
/// (the dead) skip the component removal — their row is already gone.
pub fn strip(world: &mut World, being: Entity) {
    if world.get_entity(being).is_ok() {
        world.entity_mut(being).remove::<Engaged>();
    }
    // Find the room holding this member and pull it out.
    let mut rooms: Vec<(Entity, Vec<Entity>)> = Vec::new();
    let mut query = world.query::<(Entity, &Combat)>();
    for (room, combat) in query.iter(world) {
        if combat.members.contains(&being) {
            rooms.push((room, combat.members.clone()));
        }
    }
    for (room, mut members) in rooms {
        members.retain(|&m| m != being);
        if members.is_empty() {
            world.entity_mut(room).remove::<Combat>();
        } else {
            world.entity_mut(room).insert(Combat { members });
        }
    }
}

/// Resolve a being target in the actor's room (excluding the actor): first
/// match of the BEING spec, or `None`.
pub fn find_victim(world: &mut World, actor: Entity, room: Entity, raw: &str) -> Option<Entity> {
    let spec = parse_target(raw, ParseOptions::BEING)?;
    let mut query = world.query::<(
        Entity,
        &grim_actor::InRoom,
        &GrimName,
        Option<&grim_core::components::Keywords>,
    )>();
    let here: Vec<(Entity, String, Vec<String>)> = query
        .iter(world)
        .filter(|(e, ir, _, _)| ir.room == room && *e != actor)
        .map(|(e, _, n, k)| (e, n.0.clone(), k.map(|k| k.0.clone()).unwrap_or_default()))
        .collect();
    grim_target::query(
        &spec,
        here.iter().map(|(e, n, k)| (*e, n.as_str(), k.as_slice())),
    )
    .into_iter()
    .next()
}

/// Resolve one blow from `attacker` onto `victim` (world-mut path): roll,
/// apply, emit `Damaged`. Returns whether the victim died.
pub fn strike_once(
    world: &mut World,
    attacker: Entity,
    attacker_name: &str,
    victim: Entity,
    victim_name: &str,
    kind: DamageKind,
    damage_mult: f32,
) -> bool {
    // Queued closures fire a tick after the snapshot: either side may have
    // despawned (death) since. A blow against the gone is a no-op, never a
    // panic.
    if world.get_entity(attacker).is_err() || world.get_entity(victim).is_err() {
        return false;
    }
    let att_level = world
        .get::<grim_actor::Actor>(attacker)
        .map(|a| a.level)
        .unwrap_or(1);
    let def_level = world
        .get::<grim_actor::Actor>(victim)
        .map(|a| a.level)
        .unwrap_or(1);
    let (hit, mut amount) = {
        let mut rng = world.resource_mut::<CombatRng>();
        resolve_strike(att_level, def_level, &mut rng.0)
    };
    if hit {
        amount = ((amount as f32 * damage_mult).round() as u32).max(1);
        if let Some(mut h) = world.get_mut::<Health>(victim) {
            h.damage(amount);
        }
    }
    let died = world.get::<Health>(victim).is_some_and(|h| h.is_dead());
    world
        .resource_mut::<Messages<crate::events::Damaged>>()
        .write(crate::events::Damaged {
            attacker,
            victim,
            attacker_name: attacker_name.to_string(),
            victim_name: victim_name.to_string(),
            amount,
            hit,
            kind,
        });
    died
}

/// `kill <target>`: engage a being in the actor's room and resolve the first
/// round immediately. Fail-closed on unknown targets, self, corpses, and the
/// already-dead.
pub fn handle_kill(
    mut engine: MessageReader<EngineCommand>,
    mut commands: Commands,
    inroom: Query<&grim_actor::InRoom>,
    names: Query<&GrimName>,
) {
    for cmd in engine.read() {
        let Command::Kill { target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Some(actor_room) = inroom.get(actor).ok() else {
            continue;
        };
        let Some(actor_name) = names.get(actor).ok() else {
            continue;
        };
        let target_text = target.clone();
        let room = actor_room.room;
        let attacker_name = actor_name.0.clone();
        commands.queue(move |world: &mut World| {
            let Some(victim) = find_victim(world, actor, room, &target_text) else {
                world
                    .resource_mut::<Messages<InfoMessage>>()
                    .write(InfoMessage {
                        target: actor,
                        text: tr!("combat.no_target"),
                    });
                return;
            };
            if is_corpse(world, victim) {
                world
                    .resource_mut::<Messages<InfoMessage>>()
                    .write(InfoMessage {
                        target: actor,
                        text: tr!("combat.cannot_target"),
                    });
                return;
            }
            if world.get::<Health>(victim).is_some_and(|h| h.is_dead()) {
                world
                    .resource_mut::<Messages<InfoMessage>>()
                    .write(InfoMessage {
                        target: actor,
                        text: tr!("combat.cannot_target"),
                    });
                return;
            }
            // Ensure both sides carry Health (old PCs / unseeded mobs).
            if world.get::<Health>(actor).is_none() {
                world.entity_mut(actor).insert(Health::pc());
            }
            if world.get::<Health>(victim).is_none() {
                world.entity_mut(victim).insert(Health::full(30));
            }
            // Combat auto-stands both the command path and the victim's
            // posture stays as-is (only the actor's action stands them).
            if let Some(mut p) = world.get_mut::<Posture>(actor) {
                *p = Posture::Standing;
            }
            let victim_name = world
                .get::<GrimName>(victim)
                .map(|n| n.0.clone())
                .unwrap_or_default();
            ensure_engaged(world, actor, victim, room);
            world
                .resource_mut::<Messages<FightStart>>()
                .write(FightStart {
                    being: actor,
                    name: attacker_name.clone(),
                    target: victim,
                    target_name: victim_name.clone(),
                    room,
                });
            let died = strike_once(
                world,
                actor,
                &attacker_name,
                victim,
                &victim_name,
                DamageKind::Strike,
                1.0,
            );
            if died {
                crate::death::kill_being(world, victim, Some(actor), room);
            }
        });
    }
}
/// `switch <target>`: move a mutual target to index 0 of the actor's list.
/// The candidate must be on the actor's own target list (never a bystander);
/// matching is case-insensitive name-or-keyword prefix over the listed
/// entities. Fail-closed otherwise.
pub fn handle_switch(
    mut engine: MessageReader<EngineCommand>,
    mut engaged: Query<&mut Engaged>,
    names: Query<(&GrimName, Option<&grim_core::components::Keywords>)>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Switch { target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Ok(mut list) = engaged.get_mut(actor) else {
            info.write(InfoMessage {
                target: actor,
                text: tr!("combat.not_fighting"),
            });
            continue;
        };
        let want = target.trim().to_lowercase();
        let hit = list.targets.iter().copied().find(|e| {
            names.get(*e).is_ok_and(|(n, k)| {
                n.0.to_lowercase() == want
                    || n.0.to_lowercase().starts_with(&want)
                    || k.is_some_and(|k| {
                        k.0.iter().any(|w| {
                            w.to_lowercase() == want || w.to_lowercase().starts_with(&want)
                        })
                    })
            })
        });
        match hit {
            Some(named) if list.prioritize(named) => {
                info.write(InfoMessage {
                    target: actor,
                    text: tr!("combat.switch.done"),
                });
            }
            _ => {
                info.write(InfoMessage {
                    target: actor,
                    text: tr!("combat.no_target"),
                });
            }
        }
    }
}
/// Drop stale `Combat` memberships and despawn empty rooms. Movement removes
/// the mover's own `Engaged`; this owns the room side (no cross-crate cycle).
pub fn prune(
    mut commands: Commands,
    combats: Query<(Entity, &Combat)>,
    engaged: Query<&Engaged>,
    inroom: Query<&grim_actor::InRoom>,
) {
    for (room, combat) in combats.iter() {
        let live: Vec<Entity> = combat
            .members
            .iter()
            .copied()
            .filter(|m| engaged.get(*m).is_ok() && inroom.get(*m).is_ok_and(|ir| ir.room == room))
            .collect();
        if live.len() != combat.members.len() {
            if live.is_empty() {
                commands.entity(room).remove::<Combat>();
            } else {
                commands.entity(room).insert(Combat { members: live });
            }
        }
    }
}
