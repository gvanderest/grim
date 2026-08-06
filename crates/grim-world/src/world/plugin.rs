//! `WorldPlugin`: registers the world-happening event vocabulary.
//!
//! The world owns the *kinds* of things that happen in it — a look at a room, a
//! look at an entity, a movement between rooms — even though `grim-actor`'s
//! command verbs are what trigger them. Registering these message types here
//! keeps the vocabulary with the world topology; the actor layer reads/emits
//! them and registers the input/delivery messages (`EngineCommand`,
//! `InfoMessage`, `DisconnectRequest`) it owns.

use bevy::prelude::*;
use grim_engine_types::events::{LookEntity, LookRoom, MoveEvent};

/// Registers the world-happening event vocabulary (`LookRoom`, `LookEntity`,
/// `MoveEvent`).
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LookRoom>()
            .add_message::<LookEntity>()
            .add_message::<MoveEvent>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_world_event_messages() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(WorldPlugin);
        app.update();
        assert!(app.world().get_resource::<Messages<LookRoom>>().is_some());
        assert!(app.world().get_resource::<Messages<LookEntity>>().is_some());
        assert!(app.world().get_resource::<Messages<MoveEvent>>().is_some());
    }
}
