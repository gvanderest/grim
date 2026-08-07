//! `AuthPlugin`: wires up the pre-game flow — the connection-established
//! greeter, the pre-game input dispatcher, the reserved-name and race/class
//! resources the creation flow reads, and the mis-seed guard.

use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::prelude::*;
use grim_scene::SceneSystems;
use grim_world::{ClassRegistry, RaceRegistry};

use crate::greeter::handle_connection_established;
use crate::input::handle_pregame_input;
use crate::validation::ReservedNamePrefixes;

pub struct AuthPlugin;
impl Plugin for AuthPlugin {
    fn build(&self, app: &mut App) {
        // Reserved character-name prefixes. init_resource keeps the built-in
        // defaults unless the author inserted a custom list before this plugin.
        app.init_resource::<ReservedNamePrefixes>();
        // The creation/login flow writes account/character JSON, so it needs the
        // persistence directory. init_resource so AuthPlugin stands alone; if
        // PersistencePlugin (or ScenePlugin) is also present, the identical
        // default is a no-op.
        app.init_resource::<grim_persistence::PersistenceConfig>();
        // Playable races/classes offered at character creation. init_resource so
        // the engine ships a full seed; an author overrides by inserting a custom
        // registry before adding this plugin (mirrors ReservedNamePrefixes).
        app.init_resource::<RaceRegistry>();
        app.init_resource::<ClassRegistry>();
        app.add_systems(Startup, validate_registries);
        app.add_systems(
            Update,
            (
                handle_connection_established,
                // Greeter must spawn the Client before the dispatcher reads
                // input; the dispatcher must run before the scene in-game system
                // so a line that advances a session into the world is not also
                // re-dispatched as an in-game command that same tick (see
                // input.rs module doc + JustEnteredWorld).
                handle_pregame_input
                    .after(handle_connection_established)
                    .before(SceneSystems::InGameInput),
            ),
        );
    }
}

/// Fail fast on a mis-seeded world. An empty `RaceRegistry`, or a `ClassRegistry`
/// with no tier-1 (creatable) classes, would leave character creation showing a
/// menu with no options and no exit — trapping the player. Runs at Startup, after
/// any author override has been inserted, so it validates the effective set.
fn validate_registries(races: Res<RaceRegistry>, classes: Res<ClassRegistry>) {
    assert!(
        !races.0.is_empty(),
        "RaceRegistry is empty: character creation would trap players with no race \
         to pick. Seed at least one race."
    );
    assert!(
        classes.creatable().next().is_some(),
        "ClassRegistry has no tier-1 (creatable) classes: character creation would \
         trap players. Seed at least one tier-1 class."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "RaceRegistry is empty")]
    fn empty_race_registry_panics() {
        let mut app = App::new();
        app.insert_resource(RaceRegistry(vec![]));
        app.insert_resource(ClassRegistry::default());
        app.add_systems(Startup, validate_registries);
        app.update();
    }

    #[test]
    #[should_panic(expected = "no tier-1")]
    fn class_registry_without_tier1_panics() {
        let mut app = App::new();
        app.insert_resource(RaceRegistry::default());
        app.insert_resource(ClassRegistry(vec![])); // no creatable classes
        app.add_systems(Startup, validate_registries);
        app.update();
    }
}
