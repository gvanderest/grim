//! Command events: the semantic intents emitted by the command system.
//!
//! This crate holds the **events** (not commands) that represent player intents.
//! The command system parses input and emits these events; systems that care
//! about the intent subscribe to them.
//!
//! ## Why separate commands from events?
//!
//! - **Commands** are the *parsing* layer: "the player typed 'north'"
//! - **Events** are the *semantic* layer: "the actor wants to move"
//!
//! This split keeps parsing concerns isolated from gameplay logic and allows
//! multiple parsing strategies (Telnet, SSH, WebSocket) to emit the same events.
//!
//! ## Event Categories
//!
//! The event types are grouped by intent:
//!
//! - **Movement**: [`MoveIntent`], [`LookIntent`]
//! - **Social/Channel**: [`SayIntent`], [`YellIntent`], [`OocIntent`], [`TellIntent`], [`ReplyIntent`]
//! - **Admin**: [`ShutdownIntent`], [`GotoIntent`], [`GechoIntent`]
//! - **Information**: [`WhoIntent`], [`WhereIntent`], [`CommandsIntent`], [`AreasIntent`]
//!
//! ## Usage
//!
//! Systems that care about player intents subscribe to these events:
//!
//! ```ignore
//! use bevy::prelude::*;
//! use grim_command_events::{LookIntent, MoveIntent};
//!
//! fn handle_movement(mut move_events: EventReader<MoveIntent>) {
//!     for event in move_events.read() {
//!         // Process movement intent
//!     }
//! }
//!
//! fn handle_look(mut look_events: EventReader<LookIntent>) {
//!     for event in look_events.read() {
//!         // Process look intent
//!     }
//! }
//! ```

pub mod admin;
pub mod info;
pub mod movement;
pub mod quit;
pub mod social;

// Re-export Cardinal from grim_core for use in event structs
pub use grim_core::cardinal::Cardinal;

pub use admin::*;
pub use info::*;
pub use movement::*;
pub use quit::*;
pub use social::*;
