//! Player-speech channels.
//!
//! Today this hosts the say / yell / ooc / tell / reply / gecho handlers, one
//! file per command under `commands/`. The data-driven `add_channel(Channel {
//! scope, identify, eligibility, .. })` model in ARCHITECTURE.md §7 — one
//! shared ChannelMessage event with scope-resolved audience — is deferred with
//! the typed-event command dispatch (§5.2), since it retires the distinct
//! Say/Yell/Ooc events and moves audience resolution across the grim-scene
//! render boundary.

pub mod commands;
pub mod plugin;
pub mod whisper;

pub use plugin::ChannelPlugin;
pub use whisper::LastWhisperFrom;
