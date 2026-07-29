//! Core types for GRIM MUD engine

pub mod cardinal;
pub mod color;
pub mod components;
pub mod events;
pub mod prelude;
pub mod validation;

pub use cardinal::Cardinal;
pub use color::*;
pub use prelude::*;

/// Response shown when input does not resolve to a command. Also used to mask
/// commands the actor is not privileged to run, so an unauthorized command is
/// indistinguishable from a non-existent one — no "permission denied" leak.
pub const UNKNOWN_COMMAND: &str = "Unknown command. Type 'commands' for a list.\n";
