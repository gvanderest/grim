//! `ObjectPlugin`: wires the thing verbs. Each command owns its systems and
//! message registration via a `register` fn; this plugin just calls them in
//! turn, plus registers the shared [`ItemEvent`] fact every verb emits (the
//! scene layer renders it per-recipient).

use bevy::prelude::*;
use grim_core::events::ItemEvent;

use crate::commands::{get, inventory};

/// Registers the object verbs: `get`, `drop`, `inventory`.
pub struct ObjectPlugin;

impl Plugin for ObjectPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ItemEvent>();
        get::register(app);
        inventory::register(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grim_core::events::{Command, EngineCommand, InfoMessage};

    /// The plugin composes with the world plugin and answers an inventory.
    #[test]
    fn object_plugin_registers_verbs_and_item_event() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(grim_world::WorldPlugin)
            .add_plugins(ObjectPlugin);
        app.update();

        assert!(app
            .world()
            .get_resource::<Messages<EngineCommand>>()
            .is_some());
        assert!(app
            .world()
            .get_resource::<Messages<InfoMessage>>()
            .is_some());
        assert!(app.world().get_resource::<Messages<ItemEvent>>().is_some());

        let actor = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Inventory,
        });
        app.update();
        let messages = app.world().resource::<Messages<InfoMessage>>();
        let mut cursor = messages.get_cursor();
        assert_eq!(cursor.read(messages).count(), 1);
    }
}
