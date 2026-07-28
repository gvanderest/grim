use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;

use bevy::prelude::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cardinal::Cardinal;
use crate::events::Command;

// ─── Network / Session ──────────────────────────────────────────────

/// A raw network connection. Spawned by the protocol layer.
/// The `id` maps to the tokio-side connection identifier.
#[derive(Component, Debug)]
pub struct Connection {
    pub id: usize,
    pub addr: SocketAddr,
    /// Whether the server has sent IAC WILL ECHO (hidden input) for this connection.
    /// Reset to false when the user sends their next input.
    pub echo_hidden: bool,
}

/// The client session state machine — one per connection, on a separate
/// entity from `Connection` so the engine never touches socket types.
#[derive(Component, Debug)]
pub struct Client {
    /// The `Connection` entity this client is bound to.
    pub connection: Entity,
    pub state: ClientState,
    pub account: Option<Entity>,
    pub character: Option<Entity>,
    /// Parsed commands waiting to be dispatched (cooldown-gated).
    pub input_queue: VecDeque<Command>,
    pub command_cooldown: Timer,
}

impl Client {
    pub fn new(connection: Entity) -> Self {
        Self {
            connection,
            state: ClientState::LoginPrompt,
            account: None,
            character: None,
            input_queue: VecDeque::new(),
            command_cooldown: Timer::from_seconds(0.5, TimerMode::Once),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientState {
    /// Waiting for the user to type a username or email.
    LoginPrompt,
    /// Waiting for a password. `is_new` distinguishes "create new" vs "login existing".
    /// `character` is set when the user logged in by character name — auto-select after auth.
    PasswordPrompt {
        identifier: String,
        is_new: bool,
        character: Option<Entity>,
    },
    /// "No account found. Create one? (y/n)"
    ConfirmCreate { identifier: String },
    /// Showing the character selection menu.
    CharacterSelect,
    /// Waiting for the user to type a new character name.
    CreateCharacter,
    /// MOTD prompt after character select — hit Enter to enter the world.
    MotdPrompt,
    /// In-game — input is parsed and queued as commands.
    InGame,
}

// ─── Account / Character ────────────────────────────────────────────

/// An account — persists across sessions. Saved to `data/accounts/<uuid>.json`.
#[derive(Component, Serialize, Deserialize, Clone, Debug)]
pub struct Account {
    pub id: Uuid,
    /// The user-facing identifier (username or email, normalized lowercase).
    pub identifier: String,
    pub password_hash: String,
    /// UUIDs of characters owned by this account.
    pub characters: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// A character — belongs to an account, can be in-world.
/// Saved to `data/characters/<name>.json`.
#[derive(Component, Serialize, Deserialize, Clone, Debug)]
pub struct Character {
    pub id: Uuid,
    pub name: String,
    pub account_id: Uuid,
    pub created_at: DateTime<Utc>,
    /// Last known room, persisted as (area_friendly_id, room_friendly_id).
    #[serde(default)]
    pub last_room: Option<RoomLocation>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RoomLocation {
    pub area: String,
    pub room: String,
}

// ─── World ──────────────────────────────────────────────────────────

/// An area — a collection of rooms. Friendly ID is filesystem-unique.
#[derive(Component, Debug)]
pub struct Area {
    pub id: Uuid,
    pub friendly_id: String,
    pub name: String,
}

/// A room — belongs to an area, has exits.
#[derive(Component, Debug)]
pub struct Room {
    pub id: Uuid,
    pub friendly_id: String,
    pub name: String,
    pub description: String,
    pub area: Entity,
}

/// Exits on a room entity: direction → destination room entity.
#[derive(Component, Debug, Default)]
pub struct Exits {
    pub exits: HashMap<Cardinal, Entity>,
}

/// Which room an entity is currently in.
#[derive(Component, Debug, Clone)]
pub struct InRoom {
    pub room: Entity,
}

// ─── Descriptive (shared by characters, NPCs, items) ─────────────────

/// Display name for any visible entity.
#[derive(Component, Debug, Clone)]
pub struct Name(pub String);

/// Long description shown by `look <target>`.
#[derive(Component, Debug, Clone)]
pub struct Description(pub String);

// ─── Markers ────────────────────────────────────────────────────────

/// Marks a character as player-controlled and links to their connection for output.
/// `connection: None` means the player is linkdead (disconnected but still in-world).
#[derive(Component, Debug)]
pub struct Player {
    /// The `Connection` entity to send output to, or `None` if linkdead.
    pub connection: Option<Entity>,
}

#[derive(Component, Debug, Default, Clone)]
pub struct OutputHistory {
    pub lines: std::collections::VecDeque<String>,
    pub max: usize,
}

impl OutputHistory {
    pub fn with_max(max: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(max),
            max,
        }
    }
    pub fn push(&mut self, line: &str) {
        if self.lines.len() >= self.max {
            self.lines.pop_front();
        }
        self.lines.push_back(line.to_string());
    }
    pub fn drain(&mut self) -> Vec<String> {
        self.lines.drain(..).collect()
    }
}

/// Marks an entity as an NPC.
#[derive(Component, Debug)]
pub struct Npc;

/// Character is still in-world but the player disconnected (linkdead).
#[derive(Component, Debug)]
pub struct Linkdead;
/// Inserted by the seed world system, read by the client during character creation.
#[derive(Resource, Debug)]
pub struct StartingRoom(pub Entity);
