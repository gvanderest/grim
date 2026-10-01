//! `grim-combat`: player-vs-environment combat.
//!
//! The keystone loop: `kill` a mob, trade auto-attack rounds, kill it, take
//! XP + coin + loot from its corpse. Room-scoped [`Combat`] fights, per-being
//! [`Engaged`] target lists (index 0 = primary), 5s rounds, first-round-now
//! on `kill`. Skills and spells share [`AbilityDef`]; fighting verbs are
//! `kill`/`flee`/`switch`/`kick` (plus `cast` for future spells) with
//! `sit`/`sleep`/`stand` scaling HP regen.
//!
//! Depends on `grim-actor` (beings + placement), `grim-world` (topology +
//! class grants), `grim-object` (corpses are containers), and never the
//! reverse. Output renders per-recipient in `grim-scene`.

pub mod ability;
pub mod actions;
pub mod corpse;
pub mod death;
pub mod engage;
pub mod events;
pub mod formula;
pub mod loot;
pub mod plugin;
pub mod posture;
pub mod regen;
pub mod rounds;
pub mod state;

pub use ability::{
    class_grants, AbilityCooldowns, AbilityDef, AbilityKind, AbilityRegistry, AbilityTarget,
    CombatClock,
};
pub use actions::FLEE_DELAY;
pub use corpse::{contents_in, is_corpse, Corpse, CorpseMarker};
pub use death::{kill_being, tick_corpses, CORPSE_TIMER};
pub use engage::{ensure_engaged, prune, strip, Combat};
pub use events::{DamageKind, Damaged, Died, FightStart, Fled};
pub use formula::{
    coin_for_level, death_debt, hit_chance, resolve_strike, strike_damage, xp_for_level,
};
pub use loot::{GlobalLoot, LootEntry};
pub use plugin::CombatPlugin;
pub use posture::{clear_slows, ensure_posture, tick_slows};
pub use state::{CombatRng, RegenTimer, RoundTimer};
