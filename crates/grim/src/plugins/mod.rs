pub mod persistence;
pub mod social;
pub mod world;

pub use persistence::PersistencePlugin;
pub use social::SocialPlugin;
pub use world::WorldPlugin;

use bevy::prelude::*;

/// Runs before protocol I/O — processes client input, formats output.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClientSet;

/// Runs after client processing — drains network, sends output.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProtocolSet;
