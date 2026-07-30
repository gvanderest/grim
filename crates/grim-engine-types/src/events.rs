use bevy::prelude::*;

use crate::cardinal::Cardinal;

// ─── Client → Engine ─────────────────────────────────────────────────

#[derive(Message, Debug)]
pub struct EngineCommand {
    pub client: Entity,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// `look` or `look <target>`
    Look { target: Option<String> },
    /// `say <text>` — room-scoped
    Say { text: String },
    /// `yell <text>` — area-scoped
    Yell { text: String },
    /// `ooc <text>` — global
    Ooc { text: String },
    /// Movement via cardinal direction
    Move { direction: Cardinal },
    /// `quit` — clean disconnect
    Quit,
    /// `who` — list connected players
    Who,
    /// `where` — show who's in your area and their room
    Where,
    /// `commands` — list all registered commands
    Commands,
    /// `shutdown <seconds>` — admin-only. Schedules a graceful server shutdown
    /// after a countdown, broadcasting warnings to all connected players.
    Shutdown { seconds: u64 },
}

// ─── Engine → Client (semantic events for formatting) ───────────────

/// "Show this room description to this client."
#[derive(Message, Debug)]
pub struct LookRoom {
    pub target: Entity,
    pub room: Entity,
}

/// "Show this entity's description to this client."
#[derive(Message, Debug)]
pub struct LookEntity {
    pub target: Entity,
    pub subject: Entity,
}

/// A character said something in a room. Broadcast to room occupants except the actor.
#[derive(Message, Debug)]
pub struct SayEvent {
    pub room: Entity,
    pub actor: Entity,
    pub text: String,
}

/// A character yelled something. Broadcast to all characters in the same area (rooms sharing an Area).
#[derive(Message, Debug)]
pub struct YellEvent {
    pub area: Entity,
    pub actor: Entity,
    pub text: String,
}

/// A character said something OOC (out of character). Broadcast globally.
#[derive(Message, Debug)]
pub struct OocEvent {
    pub actor: Entity,
    pub text: String,
}

/// A character moved from one room to another. Used for "X leaves north" / "X arrives" broadcasts.
#[derive(Message, Debug)]
pub struct MoveEvent {
    pub actor: Entity,
    pub from: Entity,
    pub to: Entity,
    pub direction: Cardinal,
}

/// Direct text message to a specific client (via their character entity).
#[derive(Message, Debug)]
pub struct InfoMessage {
    pub target: Entity,
    pub text: String,
}

/// A character has entered the world. Broadcast globally.
#[derive(Message, Debug)]
pub struct LoginAnnounce {
    pub name: String,
}

/// A character has left the world. Broadcast globally.
#[derive(Message, Debug)]
pub struct LogoutAnnounce {
    pub name: String,
}

/// A character went linkdead or reconnected.
#[derive(Message, Debug)]
pub struct LinkdeadAnnounce {
    pub name: String,
    pub reconnecting: bool, // true = reconnecting, false = going linkdead
}

/// An out-of-band server message shown to every connected player, regardless of
/// room or scene. Used for shutdown-countdown warnings. `text` may contain
/// colour markup and should end with `\n`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ServerBroadcast {
    pub text: String,
}
