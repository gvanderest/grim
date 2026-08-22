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
pub mod handler;
pub mod message;
pub mod plugin;
pub mod registry;
pub mod whisper;

// Re-export channel types from grim-core
pub use grim_core::channel::{Channel, Identify, ListenEligibility, Scope, SpeakEligibility};

// Re-export types from this crate
pub use crate::handler::handle_channel;
pub use crate::message::ChannelMessage;
pub use crate::registry::ChannelRegistry;

pub use plugin::ChannelPlugin;
pub use whisper::LastWhisperFrom;
