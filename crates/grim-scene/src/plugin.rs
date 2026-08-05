//! `ScenePlugin`: wires up the session-lifecycle resources, message types, and
//! systems that make up the scene subsystem.

use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::prelude::*;
use grim_engine_types::events::{
    EngineCommand, GlobalEcho, InfoMessage, LinkdeadAnnounce, LookEntity, LookRoom, MoveEvent,
    OocEvent, SayEvent, ServerBroadcast, YellEvent,
};
use grim_engine_types::events::{LoginAnnounce, LogoutAnnounce};
use grim_networking::{ConnectionOutput, ConnectionResumed, DisconnectRequest};
use grim_world::{ClassRegistry, RaceRegistry};

use crate::command::process_command_queue;
use crate::input::{handle_client_input, handle_connection_established};
use crate::output::{capture_output, format_output, format_server_broadcast};
use crate::parser;
use crate::resume::handle_connection_resumed;
use crate::validation::ReservedNamePrefixes;

pub struct ScenePlugin;
impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(parser::command_registry());
        // Scene writes account/character JSON, so it needs the persistence
        // directory. init_resource so ScenePlugin stands alone; if
        // PersistencePlugin is also present its identical default is a no-op.
        app.init_resource::<grim_persistence::PersistenceConfig>();
        // Reserved character-name prefixes. init_resource keeps the built-in
        // defaults unless the author inserted a custom list before this plugin.
        app.init_resource::<ReservedNamePrefixes>();
        // Playable races/classes offered at character creation. init_resource so
        // the engine ships a full seed; an author overrides by inserting a custom
        // registry before adding this plugin (mirrors ReservedNamePrefixes).
        app.init_resource::<RaceRegistry>();
        app.init_resource::<ClassRegistry>();
        app.add_message::<ConnectionOutput>()
            .add_message::<ConnectionResumed>()
            .add_message::<DisconnectRequest>()
            .add_message::<EngineCommand>()
            .add_message::<LookRoom>()
            .add_message::<LookEntity>()
            .add_message::<SayEvent>()
            .add_message::<YellEvent>()
            .add_message::<OocEvent>()
            .add_message::<GlobalEcho>()
            .add_message::<MoveEvent>()
            .add_message::<InfoMessage>()
            .add_message::<LoginAnnounce>()
            .add_message::<LogoutAnnounce>()
            .add_message::<LinkdeadAnnounce>()
            .add_message::<ServerBroadcast>()
            .add_systems(Startup, validate_registries)
            .add_systems(
                Update,
                (
                    handle_connection_established,
                    handle_connection_resumed,
                    handle_client_input.after(handle_connection_established),
                    process_command_queue,
                    format_output,
                    format_server_broadcast,
                    capture_output,
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
    use grim_world::{ClassRegistry, RaceRegistry};

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
