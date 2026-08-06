//! The shared "alive thing" base ([`Actor`]) carried by every being — player
//! characters and creatures alike — and the [`Creature`] marker for non-player
//! beings.
//!
//! [`Actor`] holds what movement, perception, and WHO read regardless of whether
//! the being is a PC or a mob: its race, level, and gender. A PC additionally
//! carries a [`Character`](crate::Character) (account, roles, class, …) and a
//! [`Player`](crate::Player) while connected; a mob carries a [`Creature`]
//! marker instead.

use bevy::prelude::*;
use grim_core::character::Gender;

/// The shared base every being carries — PC or creature. Movement, perception,
/// and the WHO list read race / level / gender here rather than off the
/// PC-only [`Character`](crate::Character), so the same code serves mobs.
#[derive(Component, Clone, Debug)]
pub struct Actor {
    /// Race slug (e.g. `"human"`) for a PC, or a creature's kind (may be empty).
    /// Keyed into `grim_world::RaceRegistry` for PCs; resolve leniently.
    pub race: String,
    /// Level. New characters and seeded creatures start at 1; there is no XP
    /// system yet, so this is just a stored number.
    pub level: u32,
    /// Gender — a closed set (see `grim_core::character::Gender`).
    pub gender: Gender,
}

/// Marks a being as a non-player creature (a mob). The counterpart to
/// [`Player`](crate::Player)/[`Character`](crate::Character) on PC entities: a
/// creature is `Creature + Actor + Name + InRoom` (+ optional `Description`).
///
/// Replaces the former `grim_world::Npc` marker — creatures are beings, so the
/// marker lives in the actor layer, not the being-free world.
#[derive(Component, Debug)]
pub struct Creature;
