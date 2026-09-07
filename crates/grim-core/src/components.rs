use std::collections::VecDeque;

use bevy::prelude::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::events::{Command, EditorKind};
use crate::id::GrimId;

// Re-export `Gender` so `components::*` (and, through it, the crate prelude)
// surfaces it alongside the session types. `Gender` is a creation-time choice
// referenced by `ClientState` below; the `Character` that stores it lives in
// `grim-actor`, which points *up* at this type.
pub use crate::character::Gender;

// ─── Session ────────────────────────────────────────────────────────

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
    /// The last raw input text (for "!" repeat support), excluding the "!" itself.
    pub last_input: Option<String>,
    /// Line-editor modal state. `Some` exactly while the session is inside the
    /// editor: input routes to the buffer instead of the command parser. A
    /// plain field (not a component) so opening/closing is visible to later
    /// lines in the same tick — deferred `Commands` could never do that.
    pub editor: Option<EditorSession>,
}

/// Modal line-editor state: what is edited and the lines so far. Lives on
/// [`Client::editor`]; the open/close events carry the rest.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorSession {
    pub character: Entity,
    pub kind: EditorKind,
    pub buffer: Vec<String>,
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
            last_input: None,
            editor: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientState {
    /// Waiting for the user to type a username or email.
    LoginPrompt,
    /// Waiting for a password. `is_new` distinguishes "create new" vs "login existing".
    /// `character` is the character NAME to auto-select after a correct password
    /// (set when the user logged in by character name); `None` shows the account menu.
    PasswordPrompt {
        identifier: String,
        is_new: bool,
        character: Option<String>,
    },
    /// "No account found. Create one? (y/n)"
    ConfirmCreate { identifier: String },
    /// Showing the character selection menu.
    CharacterSelect,
    /// Waiting for the user to type a new character name.
    CreateCharacter,
    /// Picking a gender for the new character. Carries the accepted `name`.
    SelectGender { name: String },
    /// Picking a race. Carries the `name` and chosen `gender`.
    SelectRace { name: String, gender: Gender },
    /// Picking a (tier-1) class. Carries the `name`, `gender`, and race `slug`.
    /// On a valid pick the character is persisted and the session advances to
    /// the MOTD.
    SelectClass {
        name: String,
        gender: Gender,
        race: String,
    },
    /// MOTD prompt after character select — hit Enter to enter the world.
    MotdPrompt,
    /// In-game — input is parsed and queued as commands.
    InGame,
}

// ─── Account ────────────────────────────────────────────────────────

/// An account — persists across sessions. Saved to `data/accounts/<id>.json`.
#[derive(Component, Serialize, Deserialize, Clone, Debug)]
pub struct Account {
    pub id: GrimId,
    /// The user-facing identifier (username or email, normalized lowercase).
    pub identifier: String,
    pub password_hash: String,
    /// Grim IDs of characters owned by this account.
    pub characters: Vec<GrimId>,
    pub created_at: DateTime<Utc>,
}

// `RoomLocation` used to live here. Placement Phase 2a step 3 relocated it to
// `grim-world` (the being-free world layer) once `Character` split, so the type
// now sits below all its consumers with no reverse edge. See
// `grim_world::RoomLocation`.

// ─── Descriptive (shared by characters, creatures, items) ────────────

/// Display name for any visible entity.
#[derive(Component, Debug, Clone)]
pub struct Name(pub String);

/// Paragraphs shown by `look <target>`, one line each: entries carry no
/// newlines and the renderer joins them with `\n`. One entry per paragraph.
#[derive(Component, Debug, Clone)]
pub struct Description(pub Vec<String>);

/// Extra lookup words for `look <keyword>` (e.g. `["grimmok", "smith"]` for
/// "Grimmok Ironhand"). Characters match by name and carry none of these;
/// creatures match by name or by any keyword prefix (case-insensitive).
#[derive(Component, Debug, Clone, Default)]
pub struct Keywords(pub Vec<String>);

/// Room-listing line for a being or object: the sentence shown under the room
/// description (e.g. `"Grimmok Ironhand stands here, hammering metal."`).
/// Distinct from [`Description`], which is the detail shown by `look <target>`.
/// Player characters do not carry this — their line is rendered from their
/// position (`"<name> is standing here."` until positions exist).
#[derive(Component, Debug, Clone)]
pub struct RoomDescription(pub String);
