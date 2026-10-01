//! Passive HP regen: 1 HP / 3s standing, 2 sitting, 4 sleeping.
//!
//! Ticks on its own `RegenTimer` (default 3000ms) so regen pacing stays
//! independent of the combat round clock.

use bevy::prelude::*;
use grim_actor::{Health, Posture};

use crate::state::RegenTimer;

/// Advance the regen clock; on expiry heal every living being by its
/// posture rate, clamped to max.
pub fn tick_regen(
    time: Res<Time>,
    mut timer: ResMut<RegenTimer>,
    mut beings: Query<(&mut Health, &Posture)>,
) {
    timer.acc += time.delta().as_secs_f32() * 1000.0;
    if timer.acc < timer.ms as f32 {
        return;
    }
    timer.acc = 0.0;
    for (mut health, posture) in beings.iter_mut() {
        if health.is_dead() {
            continue;
        }
        let amount = match posture {
            Posture::Standing => 1,
            Posture::Sitting => 2,
            Posture::Sleeping => 4,
        };
        health.heal(amount);
    }
}
