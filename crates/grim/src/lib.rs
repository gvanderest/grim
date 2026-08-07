//! GRIM — Game Runtime for Interactive Multiplayer.
//!
//! This crate is a **facade**: it depends on the subsystem crates, re-exports
//! their public surface, and offers a default plugin group. A MUD author who
//! wants the defaults depends on `grim` alone; one who wants to swap a piece
//! depends on the subsystem crates directly. Nothing here is privileged.

#![allow(ambiguous_glob_reexports)]

pub use bevy::prelude::*;
pub use grim_core::*;

// Placement Phase 1/2a relocated these domain types out of the
// `grim-core` god-node into their owning subsystem crates. Re-export
// them at the crate root (and via `prelude`) so `grim::X` / `grim::prelude::X`
// stay stable downstream.
pub mod prelude;
pub use grim_actor::{
    Actor, Character, Creature, InRoom, Linkdead, OutputHistory, Player, Role, StoredCharacter,
};
pub use grim_auth::{AuthPlugin, ReservedNamePrefixes};
pub use grim_channel::LastWhisperFrom;
pub use grim_scene::ConnectedAt;
pub use grim_world::{
    Area, ClassDef, ClassRegistry, Exits, RaceDef, RaceRegistry, Room, RoomLocation, StartingRoom,
};

// Transport-agnostic networking primitives (Connection + wire events).
pub use grim_networking::{self as networking, *};

// The text catalog. `tr` is re-exported at the crate root so `grim::tr` (the
// function) and `grim::tr!` (the macro, via #[macro_export]) both resolve.
pub use grim_text::tr;

// Command resolution.
pub use grim_command::{CommandRegistry, Contest};

pub mod plugins;
pub use plugins::*;

// The default plugin groups. Definitions live in their own module so `lib.rs`
// stays declarations + re-exports only (module-size convention).
mod plugin_groups;
pub use plugin_groups::{GrimDefaultPlugins, GrimHeadlessPlugins};
