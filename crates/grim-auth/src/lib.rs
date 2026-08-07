//! `grim-auth` — the pre-game phase (`AuthPlugin`).
//!
//! Owns the login / account-creation / character-select / MOTD flow that runs
//! before a session enters the world. Still driven by `grim_core::ClientState`
//! (the scene-stack model remains deferred — see ARCHITECTURE.md §8); it is the
//! pre-game layer above the session core in `grim-scene`.
//!
//! Cohesive concerns live in sibling modules; this file is a shell that wires
//! them together and re-exports the public plugin.

mod character_select;
mod creation;
mod finalize;
mod greeter;
mod input;
mod login;
mod params;
mod plugin;
mod world_entry;

pub mod validation;

#[cfg(test)]
mod tests;

pub use plugin::AuthPlugin;
pub use validation::ReservedNamePrefixes;
