//! World topology: rooms / areas / exits ([`topology`]) and the being-free
//! room-address lookups over them ([`area`]). The verbs that place actors in
//! this topology (`look`, `move`, `goto`) live in `grim-actor`.

mod area;
mod location;
mod plugin;
mod topology;

// Preserve the pre-split public paths (`grim_world::world::*`): these lookups
// were public here before the concern modules were carved out.
pub use area::{resolve_room_address, room_location, RoomLookup};
pub use location::RoomLocation;
pub use plugin::WorldPlugin;
// World topology types (Placement Phase 2a): re-exported at `grim_world::world::*`
// and hoisted to the crate root in `lib.rs`.
pub use topology::{Area, Exits, Room, StartingRoom};
