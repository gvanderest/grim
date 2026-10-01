//! `CombatPlugin`: the PVE combat vertical.
//!
//! Owns the fight state (`Combat` on rooms, `Engaged` on beings via
//! `grim-actor`), the round/regen clocks, abilities, loot, corpses, and the
//! combat verbs (`kill`/`flee`/`switch`/`kick`/`cast`/`sit`/`sleep`/`stand`).
//! Combat output renders per-recipient in `grim-scene`; the events it reads
//! are the facts in [`crate::events`].

use bevy::prelude::*;
use grim_core::events::{EngineCommand, InfoMessage};

use crate::ability::{seed_registry, AbilityCooldowns, AbilityRegistry, CombatClock};
use crate::death::tick_corpses;
use crate::events::{Damaged, Died, FightStart, Fled};
use crate::loot::{seed_loot, GlobalLoot};
use crate::state::{CombatRng, RegenTimer, RoundTimer};

/// Registers combat state, verbs, and clocks.
pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EngineCommand>()
            .add_message::<InfoMessage>()
            .add_message::<Damaged>()
            .add_message::<Died>()
            .add_message::<FightStart>()
            .add_message::<Fled>()
            .init_resource::<AbilityRegistry>()
            .init_resource::<AbilityCooldowns>()
            .init_resource::<CombatClock>()
            .init_resource::<GlobalLoot>()
            .init_resource::<RoundTimer>()
            .init_resource::<RegenTimer>()
            .init_resource::<CombatRng>();
        {
            let world = app.world_mut();
            seed_registry(world.resource_mut::<AbilityRegistry>().as_mut());
            seed_loot(world.resource_mut::<GlobalLoot>().as_mut());
        }
        app.add_systems(
            Update,
            (
                crate::engage::handle_kill,
                crate::engage::handle_switch,
                crate::actions::handle_flee,
                crate::actions::handle_kick,
                crate::actions::handle_cast,
                crate::posture::handle_posture,
                crate::posture::ensure_posture,
                crate::rounds::tick_rounds,
                crate::regen::tick_regen,
                crate::posture::tick_slows,
                crate::posture::clear_slows,
                crate::engage::prune,
                tick_corpses,
            ),
        );
        app.add_observer(crate::aggro::aggro_on_enter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plugin registers its messages and resources on a bare app.
    #[test]
    fn combat_plugin_registers_combat_surface() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(CombatPlugin);
        app.update();
        assert!(app.world().get_resource::<Messages<Damaged>>().is_some());
        assert!(app.world().get_resource::<Messages<Died>>().is_some());
        assert!(app.world().get_resource::<Messages<FightStart>>().is_some());
        assert!(app.world().get_resource::<AbilityRegistry>().is_some());
        assert!(app.world().get_resource::<GlobalLoot>().is_some());
        assert!(app.world().get_resource::<RoundTimer>().is_some());
        assert!(!app.world().resource::<AbilityRegistry>().0.is_empty());
        let _ = (
            crate::corpse::Corpse { timer: 1.0 },
            crate::corpse::CorpseMarker,
        );
    }
}
