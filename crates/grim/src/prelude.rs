//! The engine's prelude: the `grim-engine-types` prelude plus the domain types
//! Placement Phase 1 relocated into the subsystem crates, so downstream authors
//! keep a single `use grim::prelude::*;` surface.

pub use grim_actor::{Character, InRoom, Linkdead, OutputHistory, Player, Role};
pub use grim_channel::LastWhisperFrom;
pub use grim_engine_types::prelude::*;
pub use grim_scene::{ConnectedAt, ReservedNamePrefixes};
pub use grim_world::{
    Area, ClassDef, ClassRegistry, Exits, Npc, RaceDef, RaceRegistry, Room, StartingRoom,
};
