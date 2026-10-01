//! Engaged-move gate: degraded movement while fighting.
//!
//! Split from `movement` (file cap): the combat slow/roll gate lives here,
//! the walk orchestrator stays there. Components live in `grim-actor` so no
//! cross-crate cycle.

use std::sync::atomic::{AtomicU64, Ordering};

use bevy::prelude::*;
use grim_core::events::InfoMessage;
use grim_text::tr;

/// Per-attempt salt so consecutive engaged moves roll fresh: a pure
/// entity-seeded roll would fix one outcome per being forever.
static ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// Engaged-move gate: slowed movers are refused with `combat.delay`, fresh
/// movers take the 10% roll (slow applies on the attempt either way) and a
/// failure replies the same. True = walk; false = handled, skip. Bystanders
/// pass straight through. Factored for the line budget.
pub(crate) fn engaged_gate(
    actor: Entity,
    commands: &mut Commands,
    engaged: &Query<&crate::combat_state::Engaged>,
    slowed: &Query<&crate::combat_state::CombatSlow>,
) -> bool {
    if engaged.get(actor).is_err() {
        return true;
    }
    if slowed.get(actor).is_ok_and(|s| s.remaining > 0.0) {
        commands.queue(move |world: &mut World| {
            world
                .resource_mut::<Messages<InfoMessage>>()
                .write(InfoMessage {
                    target: actor,
                    text: tr!("combat.delay"),
                });
        });
        return false;
    }
    // 10% success per attempt: entity bits plus a process-wide attempt
    // counter, so consecutive attempts roll fresh (exact odds are
    // combat's E2E concern, not a pinned sequence).
    let salt = ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    let seed = actor
        .to_bits()
        .wrapping_add(salt)
        .wrapping_add(0x9E3779B97F4A7C15)
        .max(1);
    let mut rng = grim_core::Xorshift::seed(seed);
    commands
        .entity(actor)
        .insert(crate::combat_state::CombatSlow { remaining: 3.0 });
    if rng.chance(10) {
        return true;
    }
    commands.queue(move |world: &mut World| {
        world
            .resource_mut::<Messages<InfoMessage>>()
            .write(InfoMessage {
                target: actor,
                text: tr!("combat.delay"),
            });
    });
    false
}
