//! The default GRIM plugin groups: the headless core and the full stack.

use crate::ActorPlugin;

/// The whole GRIM engine **except a transport**: networking message wiring,
/// gameplay, and the session/scene loop. This is the shared core of
/// [`GrimDefaultPlugins`], and it is what a headless test harness composes —
/// it injects `ConnectionInput` and drains `ConnectionOutput` directly, with no
/// socket. Add a transport (or inject input) yourself.
pub struct GrimHeadlessPlugins;

impl bevy::app::PluginGroup for GrimHeadlessPlugins {
    fn build(self) -> bevy::app::PluginGroupBuilder {
        bevy::app::PluginGroupBuilder::start::<Self>()
            .add(grim_networking::GrimNetworkingPlugin)
            .add(grim_world::WorldPlugin)
            .add(grim_world::ShutdownPlugin)
            .add(ActorPlugin)
            .add(grim_channel::ChannelPlugin)
            .add(grim_persistence::PersistencePlugin)
            .add(grim_scene::ScenePlugin)
    }
}

/// The default GRIM plugin stack — the full engine a MUD author gets for free:
/// [`GrimHeadlessPlugins`] plus the telnet transport. Compose it with
/// `app.add_plugins(GrimDefaultPlugins::default())`, or list the individual
/// plugins (all re-exported under [`crate::plugins`]) to omit or replace any.
pub struct GrimDefaultPlugins {
    /// TCP port the telnet transport binds.
    pub telnet_port: u16,
}

impl Default for GrimDefaultPlugins {
    fn default() -> Self {
        Self { telnet_port: 4000 }
    }
}

impl bevy::app::PluginGroup for GrimDefaultPlugins {
    fn build(self) -> bevy::app::PluginGroupBuilder {
        GrimHeadlessPlugins
            .build()
            .add(grim_networking_telnet::TelnetPlugin::new(self.telnet_port))
    }
}
