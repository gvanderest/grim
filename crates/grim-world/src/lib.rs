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
// World topology (Placement Phase 2a): the world's static spatial types now live
// here. Hoisted to the crate root so consumers use `grim_world::{Area, Room, ...}`.
pub use world::{Area, Exits, Room, StartingRoom};
