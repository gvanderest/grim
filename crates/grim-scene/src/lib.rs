//! `grim-scene` — the session subsystem (`ScenePlugin`).
//!
//! Owns in-game input parsing/routing, output formatting + per-recipient
//! broadcast, admin-gated command dispatch, and copyover resume. The pre-game
//! flow (login / account-creation / character-select / MOTD) is layered on top
//! by the pre-game auth crate. Cohesive concerns live in sibling modules; this file is a
//! shell that wires them together and re-exports the public surface.

mod channel_output;
mod command;
mod input;
mod output;
mod params;
mod parser;
mod plugin;
mod resume;
mod scene_stack;
mod session;
mod sockets;

// Shared render helpers (MOTD text, selection menus) the pre-game auth
// flow reads. Auth → scene is allowed; scene never depends on auth.
pub mod formatter;

#[cfg(test)]
mod tests;

pub use plugin::{ScenePlugin, SceneSystems};
pub use scene_stack::{push_ingame_scene, top_is_ingame, InGameScene, SceneStack};
pub use session::{ConnectedAt, JustEnteredWorld};
