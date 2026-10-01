//! Aggression: mobs that start fights on sight.
//!
//! An [`Aggressive`] mob engages any PC entering its room (via the `Enter`
//! fact) with the same first-strike-now shape as `kill`. Passive mobs
//! (wolves) never initiate. Blueprints carry `aggressive: bool`
//! (default false); the bear seeds true.

use bevy::prelude::*;
use grim_actor::{Actor, Beings, Creature, Health, InRoom, Posture};

use crate::engage::{ensure_engaged, strike_once};
use crate::events::{DamageKind, FightStart};

/// Marker for mobs that attack PCs on room entry. Seeded per blueprint.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Aggressive;

/// Aggressive mobs placed in rooms (with health to fight with).
pub type AggroMobs<'w, 's> =
    Query<'w, 's, (Entity, &'static InRoom), (With<Creature>, With<Aggressive>, With<Health>)>;

/// A PC entered a room: every aggressive mob already there engages them with
/// an immediate first strike. Corpses, the dead, and fellow creatures never
/// trigger; already-engaged pairs just re-assert.
pub fn aggro_on_enter(
    trigger: On<grim_actor::Enter>,
    mut commands: Commands,
    mob_query: AggroMobs,
    beings: Beings,
    safe: Query<Entity, With<grim_world::SafeRoom>>,
) {
    let enter = *trigger.event();
    let (actor, room) = (enter.actor, enter.room);
    if safe.contains(room) {
        return;
    }
    // Only PCs trigger aggression.
    let Ok((_, _, name, _, pc, creature, _, _)) = beings.get(actor) else {
        return;
    };
    if pc.is_none() || creature.is_some() {
        return;
    }
    let actor_name = name.0.clone();
    for (mob, mob_room) in mob_query.iter() {
        if mob_room.room != room || mob == actor {
            continue;
        }
        let Ok((_, _, mob_name, _, _, _, _, _)) = beings.get(mob) else {
            continue;
        };
        let mob_name = mob_name.0.clone();
        // Dead mobs and corpses never aggro.
        let (actor_name, mob_name) = (actor_name.clone(), mob_name.clone());
        commands.queue(move |world: &mut World| {
            if world.get_entity(mob).is_err() || world.get_entity(actor).is_err() {
                return;
            }
            if world.get::<Health>(mob).is_some_and(|h| h.is_dead()) {
                return;
            }
            if world.get::<Health>(actor).is_some_and(|h| h.is_dead()) {
                return;
            }
            if world.get::<Health>(mob).is_none() {
                world.entity_mut(mob).insert(Health::full(30));
            }
            if world.get::<Health>(actor).is_none() {
                world.entity_mut(actor).insert(Health::pc());
            }
            let mob_level = world.get::<Actor>(mob).map(|a| a.level).unwrap_or(1);
            let _ = mob_level;
            ensure_engaged(world, mob, actor, room);
            world
                .resource_mut::<Messages<FightStart>>()
                .write(FightStart {
                    being: mob,
                    name: mob_name.clone(),
                    target: actor,
                    target_name: actor_name.clone(),
                    room,
                });
            let died = strike_once(
                world,
                mob,
                &mob_name,
                actor,
                &actor_name,
                DamageKind::Strike,
                1.0,
            );
            if died {
                crate::death::kill_being(world, actor, Some(mob), room);
            }
        });
        let _ = Posture::Standing;
    }
}
