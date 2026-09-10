//! Plugin re-exports. The gameplay plugins live in their own crates
//! (grim-world, grim-persistence, grim-channel); the facade re-exports them so
//! `grim::plugins::WorldPlugin` etc. keep resolving.

pub use grim_actor::ActorPlugin;
pub use grim_auth::AuthPlugin;
pub use grim_channel::ChannelPlugin;
pub use grim_networking::GrimNetworkingPlugin;
pub use grim_networking_telnet::TelnetPlugin;
pub use grim_object::ObjectPlugin;
pub use grim_persistence::{BanEntry, BanList, PersistenceConfig, PersistencePlugin};
pub use grim_scene::ScenePlugin;
pub use grim_script::ScriptPlugin;
pub use grim_social::SocialPlugin;
pub use grim_world::{ShutdownPlugin, WorldPlugin};
