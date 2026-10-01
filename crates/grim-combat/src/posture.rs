//! Posture and slow ticks: sit/sleep/stand, CombatSlow decay, posture backfill.
//!
//! Split from `actions` (file cap): verbs stay there, body-state ticks live here.

use bevy::prelude::*;
use grim_actor::{CombatSlow, Engaged, Health, Posture};
use grim_core::components::Name as GrimName;
use grim_core::events::{Command, EngineCommand, InfoMessage};
use grim_text::tr;

use crate::ability::{AbilityTarget, CombatClock};
use crate::corpse::is_corpse;

/// `sit` / `sleep` / `stand`: set posture. Allowed anywhere (in or out of
/// combat); fail-closed when already in the requested posture.
pub fn handle_posture(
    mut engine: MessageReader<EngineCommand>,
    mut posture: Query<&mut Posture>,
    names: Query<&GrimName>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let (actor, want, already_key, done_key) = match &cmd.command {
            Command::Sit => (
                cmd.client,
                Posture::Sitting,
                "combat.already_sitting",
                "combat.sit",
            ),
            Command::Sleep => (
                cmd.client,
                Posture::Sleeping,
                "combat.already_sleeping",
                "combat.sleep",
            ),
            Command::Stand => (
                cmd.client,
                Posture::Standing,
                "combat.already_standing",
                "combat.stand",
            ),
            _ => continue,
        };
        let Ok(mut p) = posture.get_mut(actor) else {
            continue;
        };
        let _ = names;
        if *p == want {
            info.write(InfoMessage {
                target: actor,
                text: tr!(already_key),
            });
        } else {
            *p = want;
            info.write(InfoMessage {
                target: actor,
                text: tr!(done_key),
            });
        }
    }
}

/// Tick `CombatSlow` down and advance the combat clock (cooldown basis).
pub fn tick_slows(
    mut slows: Query<&mut CombatSlow>,
    time: Res<Time>,
    mut clock: ResMut<CombatClock>,
) {
    clock.0 += time.delta().as_secs_f32();
    for mut slow in slows.iter_mut() {
        slow.remaining = (slow.remaining - time.delta().as_secs_f32()).max(0.0);
    }
}

/// Remove expired slows so queries for "slowed" stay cheap.
pub fn clear_slows(mut commands: Commands, slows: Query<(Entity, &CombatSlow)>) {
    for (entity, slow) in slows.iter() {
        if slow.remaining <= 0.0 {
            commands.entity(entity).remove::<CombatSlow>();
        }
    }
}

/// Ensure every live being carries `Posture` (old PCs / unseeded mobs).
pub fn ensure_posture(
    mut commands: Commands,
    beings: Query<Entity, (Without<Posture>, With<Health>)>,
) {
    for entity in beings.iter() {
        commands.entity(entity).insert(Posture::Standing);
    }
}

/// The legitimate-target gate for future abilities: Foe requires mutual
/// engagement in the same room.
#[allow(dead_code)]
pub fn target_ok(
    world: &mut World,
    user: Entity,
    target: Entity,
    want: AbilityTarget,
    room: Entity,
) -> bool {
    match want {
        AbilityTarget::OwnSelf => user == target,
        AbilityTarget::Foe => {
            world
                .get::<Engaged>(user)
                .is_some_and(|e| e.targets.contains(&target))
                && world
                    .get::<grim_actor::InRoom>(target)
                    .is_some_and(|ir| ir.room == room)
                && !is_corpse(world, target)
                && !world.get::<Health>(target).is_some_and(|h| h.is_dead())
        }
        AbilityTarget::Ally | AbilityTarget::Any => world
            .get::<grim_actor::InRoom>(target)
            .is_some_and(|ir| ir.room == room),
    }
}
