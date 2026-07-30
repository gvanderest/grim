//! World subsystem: rooms, areas, exits, movement, `look`, plus server-control
//! commands (`shutdown` and the SIGUSR1 graceful-shutdown bridge).

pub mod shutdown;
pub mod world;

pub use shutdown::ShutdownPlugin;
pub use world::WorldPlugin;
