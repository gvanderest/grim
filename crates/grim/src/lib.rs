//! GRIM - Game Runtime for Interactive Multiplayer
//!
//! This crate provides a compatibility layer that re-exports from grim-engine-types.
//! The actual types live in grim-engine-types for better separation of concerns.

#![allow(ambiguous_glob_reexports)]

pub use bevy::prelude::*;
pub use grim_engine_types::*;

pub mod plugins;

pub use plugins::*;
