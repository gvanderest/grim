//! `grim-scene` — the session subsystem (`ScenePlugin`).
//!
//! Owns session lifecycle (`ClientState`), input parsing/routing, output
//! formatting + per-recipient broadcast, admin-gated command dispatch, and
//! copyover resume. Cohesive concerns live in sibling modules; this file is a
//! shell that wires them together and re-exports the public plugin.

mod character;
mod command;
mod creation;
mod formatter;
mod input;
mod login;
mod output;
mod params;
mod parser;
mod plugin;
mod resume;
mod world_entry;

#[cfg(test)]
mod tests;

pub use plugin::ScenePlugin;
