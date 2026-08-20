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

use bevy::prelude::*;

// ─── Movement ────────────────────────────────────────────────────────────────

/// An actor wants to move in a direction.
#[derive(Message, Debug)]
pub struct MoveIntent {
    pub actor: Entity,
    pub direction: grim_core::Cardinal,
}

/// An actor wants to look at something (room or entity).
#[derive(Message, Debug)]
pub struct LookIntent {
    pub actor: Entity,
    pub target: Option<String>,
}

/// An actor wants to quit.
#[derive(Message, Debug)]
pub struct QuitIntent {
    pub actor: Entity,
}

// ─── Social/Channel Commands ─────────────────────────────────────────────────

/// An actor wants to say something (room-scoped).
#[derive(Message, Debug)]
pub struct SayIntent {
    pub actor: Entity,
    pub text: String,
}

/// An actor wants to yell something (area-scoped).
#[derive(Message, Debug)]
pub struct YellIntent {
    pub actor: Entity,
    pub text: String,
}

/// An actor wants to say something OOC (global).
#[derive(Message, Debug)]
pub struct OocIntent {
    pub actor: Entity,
    pub text: String,
}

/// An actor wants to whisper to another player.
#[derive(Message, Debug)]
pub struct TellIntent {
    pub actor: Entity,
    pub target: String,
    pub text: String,
}

/// An actor wants to reply to the last whisper sender.
#[derive(Message, Debug)]
pub struct ReplyIntent {
    pub actor: Entity,
    pub text: String,
}

// ─── Admin Commands ──────────────────────────────────────────────────────────

/// An admin wants to schedule a shutdown.
#[derive(Message, Debug)]
pub struct ShutdownIntent {
    pub actor: Entity,
    pub seconds: u64,
}

/// An admin wants to teleport to a room.
#[derive(Message, Debug)]
pub struct GotoIntent {
    pub actor: Entity,
    pub target: String,
}

/// An admin wants to broadcast to all players.
#[derive(Message, Debug)]
pub struct GechoIntent {
    pub actor: Entity,
    pub text: String,
}

// ─── Info Commands ───────────────────────────────────────────────────────────

/// An actor wants to see the WHO list.
#[derive(Message, Debug)]
pub struct WhoIntent {
    pub actor: Entity,
}

/// An actor wants to see who's in their area.
#[derive(Message, Debug)]
pub struct WhereIntent {
    pub actor: Entity,
}

/// An actor wants to see the commands list.
#[derive(Message, Debug)]
pub struct CommandsIntent {
    pub actor: Entity,
}

/// An actor wants to see the areas list.
#[derive(Message, Debug)]
pub struct AreasIntent {
    pub actor: Entity,
}
