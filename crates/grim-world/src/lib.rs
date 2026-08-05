//! World subsystem: rooms, areas, exits, movement, `look`, plus server-control
//! commands (`shutdown` and the SIGUSR1 graceful-shutdown bridge).

pub mod npc;
pub mod registry;
pub mod shutdown;
pub mod world;

pub use npc::Npc;
pub use registry::{ClassDef, ClassRegistry, RaceDef, RaceRegistry};
pub use shutdown::ShutdownPlugin;
pub use world::WorldPlugin;
