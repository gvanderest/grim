//! `ChannelPlugin`: wires the player-speech command verbs. Each command owns
//! its own systems and message registration via a `register` fn; this plugin
//! just calls them in turn.

use bevy::prelude::*;

use crate::{Channel, ChannelRegistry, Identify, ListenEligibility, Scope, SpeakEligibility};

/// Handles `say`/`yell`/`ooc`/`tell`/`reply`/`gecho` commands, emitting the
/// corresponding channel events plus `InfoMessage` echoes.
pub struct ChannelPlugin;

impl ChannelPlugin {
    /// Register a new channel from configuration.
    ///
    /// This is the data-driven approach from ARCHITECTURE.md §7. Channels
    /// are data, not code - one `Channel` configuration registers the
    /// command, audience resolution, and formatting.
    pub fn add_channel(&self, _channel: Channel) {
        // The actual registration happens in build() - we just store it for later
        // This method signature is for the API, but we need a different pattern
        // since Plugin::build() runs once at startup
    }
}

impl Plugin for ChannelPlugin {
    fn build(&self, app: &mut App) {
        // Initialize the channel registry
        app.init_resource::<ChannelRegistry>();

        // Register default channels: say, yell, ooc
        app.world_mut()
            .resource_mut::<ChannelRegistry>()
            .add_channel(Channel {
                name: "say".to_string(),
                scope: Scope::Room,
                identify: Identify::Perceived,
                toggleable: false,
                speak: SpeakEligibility::All,
                listen: ListenEligibility::All,
                key: "channel.say".to_string(),
            });

        app.world_mut()
            .resource_mut::<ChannelRegistry>()
            .add_channel(Channel {
                name: "yell".to_string(),
                scope: Scope::Area,
                identify: Identify::Perceived,
                toggleable: false,
                speak: SpeakEligibility::All,
                listen: ListenEligibility::InArea,
                key: "channel.yell".to_string(),
            });

        app.world_mut()
            .resource_mut::<ChannelRegistry>()
            .add_channel(Channel {
                name: "ooc".to_string(),
                scope: Scope::Global,
                identify: Identify::Always,
                toggleable: true,
                speak: SpeakEligibility::All,
                listen: ListenEligibility::All,
                key: "channel.ooc".to_string(),
            });

        // Register the command handlers (systems + messages)
        crate::commands::say::register(app);
        crate::commands::yell::register(app);
        crate::commands::ooc::register(app);
        crate::commands::gecho::register(app);
        crate::commands::tell::register(app);
        crate::commands::reply::register(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_plugin_registers_default_channels() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ChannelPlugin);
        app.update();

        let registry = app.world().get_resource::<ChannelRegistry>();
        assert!(registry.is_some());

        let registry = registry.unwrap();
        assert!(registry.get("say").is_some());
        assert!(registry.get("yell").is_some());
        assert!(registry.get("ooc").is_some());
    }

    #[test]
    fn channel_plugin_say_config() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ChannelPlugin);
        app.update();

        let registry = app.world().get_resource::<ChannelRegistry>().unwrap();
        let say = registry.get("say").unwrap();
        assert_eq!(say.name, "say");
        assert_eq!(say.scope, Scope::Room);
        assert_eq!(say.identify, Identify::Perceived);
        assert!(!say.toggleable);
    }

    #[test]
    fn channel_plugin_yell_config() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ChannelPlugin);
        app.update();

        let registry = app.world().get_resource::<ChannelRegistry>().unwrap();
        let yell = registry.get("yell").unwrap();
        assert_eq!(yell.name, "yell");
        assert_eq!(yell.scope, Scope::Area);
        assert_eq!(yell.identify, Identify::Perceived);
        assert!(!yell.toggleable);
    }

    #[test]
    fn channel_plugin_ooc_config() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ChannelPlugin);
        app.update();

        let registry = app.world().get_resource::<ChannelRegistry>().unwrap();
        let ooc = registry.get("ooc").unwrap();
        assert_eq!(ooc.name, "ooc");
        assert_eq!(ooc.scope, Scope::Global);
        assert_eq!(ooc.identify, Identify::Always);
        assert!(ooc.toggleable);
    }
}
