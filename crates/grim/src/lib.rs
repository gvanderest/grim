//! GRIM - Game Runtime for Interactive Multiplayer
//!
//! This crate provides a compatibility layer that re-exports from grim-engine-types.
//! The actual types live in grim-engine-types for better separation of concerns.

#![allow(ambiguous_glob_reexports)]

pub use bevy::prelude::*;
pub use grim_engine_types::*;

// The text catalog. `tr` is re-exported at the crate root so `grim::tr` (the
// function) and `grim::tr!` (the macro, via #[macro_export]) both resolve, as
// the old `tr!` in grim-engine-types did.
pub use grim_text::tr;

// Command resolution moved to grim-command; re-export so `grim::CommandRegistry`
// keeps resolving.
pub use grim_command::{CommandRegistry, Contest};

pub mod plugins;

pub use plugins::*;
