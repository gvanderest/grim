//! Combat facts: what happened, rendered per-recipient by `grim-scene`.
//!
//! Fact-only vocabulary (no vetoable attempt yet — see the design): strikes
//! land or miss, fights start, flees resolve, beings die. Names are
//! precomputed so renderers need no lookups.

use bevy::prelude::*;

/// Which kind of blow a [`Damaged`] fact reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageKind {
    Strike,
    Kick,
}

/// One auto-attack (or kick) resolved against a victim. `hit == false` means a
/// clean miss (`amount` is then 0). `damage_noun`/`target_noun` render the
/// blow: the attacker's unarmed noun (players: `punch`) against the victim's
/// noun (creatures: per-blueprint, default `bite`).
#[derive(Message, Debug, Clone)]
pub struct Damaged {
    pub attacker: Entity,
    pub victim: Entity,
    pub attacker_name: String,
    pub victim_name: String,
    pub amount: u32,
    pub hit: bool,
    pub kind: DamageKind,
    pub damage_noun: String,
    pub target_noun: String,
}

/// A being died. `xp`/`coin` are the awards already banked on the killer's
/// `Character` (0 for PC deaths, which instead take an XP debt).
#[derive(Message, Debug, Clone)]
pub struct Died {
    pub victim: Entity,
    pub victim_name: String,
    pub killer: Option<Entity>,
    pub killer_name: Option<String>,
    pub room: Entity,
    pub victim_pc: bool,
    pub killer_pc: bool,
    pub xp: u32,
    pub coin: u32,
}

/// Two beings are now mutually engaged in `room`.
#[derive(Message, Debug, Clone)]
pub struct FightStart {
    pub being: Entity,
    pub name: String,
    pub target: Entity,
    pub target_name: String,
    pub room: Entity,
}

/// A flee attempt resolved. `success == false` leaves the being engaged.
/// On success `direction`/`dest` carry where the being went (for the
/// "flee to the …" lines and the arrival look).
#[derive(Message, Debug, Clone)]
pub struct Fled {
    pub being: Entity,
    pub name: String,
    pub room: Entity,
    pub success: bool,
    pub direction: Option<grim_core::Cardinal>,
    pub dest: Option<Entity>,
}
