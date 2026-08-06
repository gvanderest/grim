//! `ActorPlugin`: wires the being-reading command verbs. Each command owns its
//! own systems and message registration via a `register` fn; this plugin just
//! calls them in turn.
//!
//! The world-happening events these verbs emit (`LookRoom`, `LookEntity`,
//! `MoveEvent`) are registered by `grim_world::WorldPlugin`, and the shutdown
//! countdown/signal machinery by `grim_world::ShutdownPlugin` — both of which a
//! full stack composes alongside this plugin.

use bevy::prelude::*;

use crate::commands;

/// Registers the actor command verbs: `look`, `move`/`goto`, `quit`, `title`,
/// and the admin `shutdown` gate.
pub struct ActorPlugin;

impl Plugin for ActorPlugin {
    fn build(&self, app: &mut App) {
        commands::look::register(app);
        commands::movement::register(app);
        commands::quit::register(app);
        commands::title::register(app);
        commands::shutdown::register(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grim_engine_types::events::{
        Command, EngineCommand, InfoMessage, LookEntity, LookRoom, MoveEvent,
    };
    use grim_networking::DisconnectRequest;

    /// The plugin composes with the world plugins it leans on, and registers the
    /// messages every verb reads/emits.
    #[test]
    fn actor_plugin_registers_verb_messages() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(grim_world::WorldPlugin)
            .add_plugins(grim_world::ShutdownPlugin)
            .add_plugins(ActorPlugin);
        app.update();

        // Input + delivery messages the actor owns.
        assert!(app
            .world()
            .get_resource::<Messages<EngineCommand>>()
            .is_some());
        assert!(app
            .world()
            .get_resource::<Messages<InfoMessage>>()
            .is_some());
        assert!(app
            .world()
            .get_resource::<Messages<DisconnectRequest>>()
            .is_some());
        // World-happening events the verbs emit (registered by WorldPlugin).
        assert!(app.world().get_resource::<Messages<LookRoom>>().is_some());
        assert!(app.world().get_resource::<Messages<LookEntity>>().is_some());
        assert!(app.world().get_resource::<Messages<MoveEvent>>().is_some());
    }

    /// End-to-end through the composed plugin: a `look` with no target routes to
    /// a `LookRoom` for the actor's room.
    #[test]
    fn look_verb_runs_under_the_plugin() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(grim_world::WorldPlugin)
            .add_plugins(grim_world::ShutdownPlugin)
            .add_plugins(ActorPlugin);
        let room = app.world_mut().spawn(()).id();
        let actor = app
            .world_mut()
            .spawn(crate::placement::InRoom { room })
            .id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Look { target: None },
        });
        app.update();
        let messages = app.world().resource::<Messages<LookRoom>>();
        let mut cursor = messages.get_cursor();
        assert_eq!(cursor.read(messages).count(), 1);
    }
}
