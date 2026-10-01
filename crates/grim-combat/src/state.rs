//! Combat resources: round pacing, regen pacing, RNG.

use bevy::prelude::*;
use grim_core::Xorshift;

/// Milliseconds between auto-attack rounds. Tunable; default 5000.
#[derive(Resource, Debug, Clone, Copy)]
pub struct RoundTimer {
    pub ms: u64,
    pub acc: f32,
}

impl Default for RoundTimer {
    fn default() -> Self {
        Self { ms: 5000, acc: 0.0 }
    }
}

/// Milliseconds between HP regen ticks. Default 3000.
#[derive(Resource, Debug, Clone, Copy)]
pub struct RegenTimer {
    pub ms: u64,
    pub acc: f32,
}

impl Default for RegenTimer {
    fn default() -> Self {
        Self { ms: 3000, acc: 0.0 }
    }
}

/// The combat RNG. Seeded once; tests overwrite with a fixed seed.
#[derive(Resource)]
pub struct CombatRng(pub Xorshift);

impl Default for CombatRng {
    fn default() -> Self {
        Self(Xorshift::seed(0xC0FFEE))
    }
}
