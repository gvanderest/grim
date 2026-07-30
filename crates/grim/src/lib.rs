//! GRIM — Game Runtime for Interactive Multiplayer.
//!
//! This crate is a **facade**: it depends on the subsystem crates, re-exports
//! their public surface, and offers a default plugin group. A MUD author who
//! wants the defaults depends on `grim` alone; one who wants to swap a piece
//! depends on the subsystem crates directly. Nothing here is privileged.

#![allow(ambiguous_glob_reexports)]

pub use bevy::prelude::*;
pub use grim_engine_types::*;

// Transport-agnostic networking primitives (Connection + wire events).
pub use grim_networking::{self as networking, *};

// The text catalog. `tr` is re-exported at the crate root so `grim::tr` (the
// function) and `grim::tr!` (the macro, via #[macro_export]) both resolve.
pub use grim_text::tr;

// Command resolution.
pub use grim_command::{CommandRegistry, Contest};

pub mod plugins;
pub use plugins::*;

/// The default GRIM plugin stack — the full engine a MUD author gets for free.
/// Compose it with `app.add_plugins(GrimDefaultPlugins::default())`, or list the
/// individual plugins (all re-exported under [`plugins`]) to omit or replace any.
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
        bevy::app::PluginGroupBuilder::start::<Self>()
            .add(grim_world::WorldPlugin)
            .add(grim_world::ShutdownPlugin)
            .add(grim_channel::ChannelPlugin)
            .add(grim_persistence::PersistencePlugin)
            .add(grim_scene::ScenePlugin)
            .add(grim_networking_telnet::TelnetPlugin::new(self.telnet_port))
    }
}
