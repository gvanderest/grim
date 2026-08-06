//! World subsystem: the static spatial structure of the game — rooms, areas,
//! exits, and the shared room-address lookups over them — plus the graceful
//! server-shutdown signal/countdown machinery (`ShutdownPlugin`).
//!
//! This crate is **being-free**: it knows the rooms, not who stands in them.
//! The beings (`Character`, `Player`, `InRoom`, …) and the verbs that read them
//! (`look`, `move`, `goto`, `quit`, `title`, and the admin `shutdown` gate) live
//! in `grim-actor`, which depends on this crate — never the reverse.

pub mod npc;
pub mod registry;
pub mod shutdown;
pub mod world;

pub use npc::Npc;
pub use registry::{ClassDef, ClassRegistry, RaceDef, RaceRegistry};
pub use shutdown::{ShutdownPlugin, ShutdownSet};
pub use world::WorldPlugin;
// World topology (Placement Phase 2a): the world's static spatial types now live
// here. Hoisted to the crate root so consumers use `grim_world::{Area, Room, ...}`.
pub use world::{Area, Exits, Room, StartingRoom};
// Shared room-address lookups (being-free): the actor movement verbs resolve
// destinations through these. Hoisted so consumers use `grim_world::{...}`.
pub use world::{resolve_room_address, room_location, RoomLookup};
