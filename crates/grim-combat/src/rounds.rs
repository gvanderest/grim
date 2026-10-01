//! Round engine: every `RoundTimer` ms, each engaged being strikes its
//! primary (index 0). Prunes invalid primaries first (dead, moved rooms,
//! disengaged); kills route through `kill_being`.

use bevy::prelude::*;
use grim_actor::{Engaged, Health, InRoom};

use crate::corpse::is_corpse;
use crate::engage::{ensure_engaged, strike_once};
use crate::events::DamageKind;
use crate::state::RoundTimer;

/// Advance the round clock; on expiry every engaged being auto-attacks its
/// primary target.
pub fn tick_rounds(
    time: Res<Time>,
    mut timer: ResMut<RoundTimer>,
    combats: Query<(Entity, &crate::engage::Combat)>,
    engaged: Query<&Engaged>,
    inroom: Query<&InRoom>,
    health: Query<&Health>,
    mut commands: Commands,
) {
    timer.acc += time.delta().as_secs_f32() * 1000.0;
    if timer.acc < timer.ms as f32 {
        return;
    }
    timer.acc = 0.0;
    // Snapshot the pairs so the queued closures below see a stable list.
    let mut pairs: Vec<(Entity, Entity, Entity)> = Vec::new();
    for (room, combat) in combats.iter() {
        for &member in &combat.members {
            let Ok(list) = engaged.get(member) else {
                continue;
            };
            let Some(primary) = list.primary() else {
                continue;
            };
            // Prune: both sides engaged, sharing the room, alive, not corpses.
            if engaged.get(primary).is_err() {
                continue;
            }
            if inroom.get(member).is_ok_and(|ir| ir.room != room) {
                continue;
            }
            if inroom.get(primary).is_ok_and(|ir| ir.room != room) {
                continue;
            }
            if health.get(member).is_ok_and(|h| h.is_dead()) {
                continue;
            }
            if health.get(primary).is_ok_and(|h| h.is_dead()) {
                continue;
            }
            pairs.push((member, primary, room));
        }
    }
    for (attacker, victim, room) in pairs {
        commands.queue(move |world: &mut World| {
            if is_corpse(world, victim) || is_corpse(world, attacker) {
                return;
            }
            if world.get::<Health>(attacker).is_some_and(|h| h.is_dead()) {
                return;
            }
            if world.get::<Health>(victim).is_some_and(|h| h.is_dead()) {
                return;
            }
            // Attacking re-asserts mutual engagement (covers joiners).
            ensure_engaged(world, attacker, victim, room);
            let attacker_name = world
                .get::<grim_core::components::Name>(attacker)
                .map(|n| n.0.clone())
                .unwrap_or_default();
            let victim_name = world
                .get::<grim_core::components::Name>(victim)
                .map(|n| n.0.clone())
                .unwrap_or_default();
            let died = strike_once(
                world,
                attacker,
                &attacker_name,
                victim,
                &victim_name,
                DamageKind::Strike,
                1.0,
            );
            if died {
                crate::death::kill_being(world, victim, Some(attacker), room);
            }
        });
    }
    let _ = commands;
}
