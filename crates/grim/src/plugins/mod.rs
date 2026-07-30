//! Plugin re-exports. The gameplay plugins live in their own crates
//! (grim-world, grim-persistence, grim-channel); the facade re-exports them so
//! `grim::plugins::WorldPlugin` etc. keep resolving.

pub use grim_channel::ChannelPlugin;
pub use grim_networking_telnet::TelnetPlugin;
pub use grim_persistence::PersistencePlugin;
pub use grim_scene::ScenePlugin;
pub use grim_world::{ShutdownPlugin, WorldPlugin};
