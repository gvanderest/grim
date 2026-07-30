//! Plugin re-exports. World, shutdown, and persistence now live in their own
//! crates (grim-world, grim-persistence); the facade re-exports them so
//! `grim::plugins::WorldPlugin` etc. keep resolving. `social` still lives here
//! until it becomes grim-channel.

pub mod social;

pub use grim_persistence::PersistencePlugin;
pub use grim_world::{ShutdownPlugin, WorldPlugin};
pub use social::SocialPlugin;
