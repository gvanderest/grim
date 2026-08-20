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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_intent_docstring_is_valid() {
        // Test that the module docstring is accessible
        let doc = "Command events: the semantic intents emitted by the command system.";
        assert!(doc.contains("Command events"));
    }

    #[test]
    fn all_intents_implement_message() {
        // Verify all types can be used as Bevy Messages
        fn assert_message<T: bevy::prelude::Message>() {}
        assert_message::<MoveIntent>();
        assert_message::<LookIntent>();
        assert_message::<QuitIntent>();
        assert_message::<SayIntent>();
        assert_message::<YellIntent>();
        assert_message::<OocIntent>();
        assert_message::<TellIntent>();
        assert_message::<ReplyIntent>();
        assert_message::<ShutdownIntent>();
        assert_message::<GotoIntent>();
        assert_message::<GechoIntent>();
        assert_message::<WhoIntent>();
        assert_message::<WhereIntent>();
        assert_message::<CommandsIntent>();
        assert_message::<AreasIntent>();
    }

    #[test]
    fn all_intents_implement_debug() {
        // Verify all types implement Debug - this verifies the derive macro works
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<MoveIntent>();
        assert_debug::<LookIntent>();
        assert_debug::<QuitIntent>();
        assert_debug::<SayIntent>();
        assert_debug::<YellIntent>();
        assert_debug::<OocIntent>();
        assert_debug::<TellIntent>();
        assert_debug::<ReplyIntent>();
        assert_debug::<ShutdownIntent>();
        assert_debug::<GotoIntent>();
        assert_debug::<GechoIntent>();
        assert_debug::<WhoIntent>();
        assert_debug::<WhereIntent>();
        assert_debug::<CommandsIntent>();
        assert_debug::<AreasIntent>();
    }
}
