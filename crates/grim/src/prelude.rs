//! The engine's prelude: the `grim-core` prelude plus the domain types
//! Placement Phase 1 relocated into the subsystem crates, so downstream authors
//! keep a single `use grim::prelude::*;` surface.

pub use grim_actor::{
    Actor, Character, Creature, InRoom, Linkdead, OutputHistory, Player, Role, StoredCharacter,
};
pub use grim_channel::LastWhisperFrom;
pub use grim_core::prelude::*;
pub use grim_scene::{ConnectedAt, ReservedNamePrefixes};
pub use grim_world::{
    Area, ClassDef, ClassRegistry, Exits, RaceDef, RaceRegistry, Room, RoomLocation, StartingRoom,
};
