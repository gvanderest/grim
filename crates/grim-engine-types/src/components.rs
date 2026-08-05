use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cardinal::Cardinal;
use crate::events::Command;
use crate::id::GrimId;

// Re-export the character build-data types so `components::*` (and, through it,
// the crate prelude) surfaces them alongside `Character` itself. The bare
// `Gender` name here also types the `Character::gender` field below.
pub use crate::character::{ClassDef, ClassRegistry, Gender, RaceDef, RaceRegistry};

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

// ─── Account / Character ────────────────────────────────────────────

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

/// A privilege a character holds. Serialized lowercase (`"admin"`) so the
/// character JSON stays human-editable — granting admin is a manual JSON edit
/// today (see `docs/DEPLOY.md`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
}

/// A character — belongs to an account, can be in-world.
/// Saved to `data/characters/<name>.json`.
#[derive(Component, Serialize, Deserialize, Clone, Debug)]
pub struct Character {
    pub id: GrimId,
    pub name: String,
    pub account_id: GrimId,
    pub created_at: DateTime<Utc>,
    /// Last known room, persisted as (area_friendly_id, room_friendly_id).
    #[serde(default)]
    pub last_room: Option<RoomLocation>,
    /// Privileges. Empty for normal players. `#[serde(default)]` keeps old
    /// character JSON (written before roles existed) loading cleanly.
    #[serde(default)]
    pub roles: Vec<Role>,
    /// Chosen at creation. `#[serde(default)]` → [`Gender::Neutral`] for old
    /// character JSON written before genders existed.
    #[serde(default)]
    pub gender: Gender,
    /// Race slug (e.g. `"human"`), keyed into [`RaceRegistry`]. Empty on old
    /// JSON (and never picked); resolve leniently.
    #[serde(default)]
    pub race: String,
    /// Class slug (e.g. `"warrior"`), keyed into [`ClassRegistry`]. Holds a
    /// single tier-1 slug for now; a future reroll swaps it to the tier-2
    /// `evolves_to` (see `docs/adr/0002-character-class-tiers.md`).
    #[serde(default)]
    pub class: String,
    /// Character level. New characters start at 1; there is no XP system yet,
    /// so this is just a stored number. `#[serde(default = ...)]` gives old
    /// JSON level 1 rather than 0.
    #[serde(default = "default_level")]
    pub level: u32,
}

/// The level every new (or pre-level-field) character starts at.
fn default_level() -> u32 {
    1
}

impl Character {
    /// Whether this character holds the admin role.
    pub fn is_admin(&self) -> bool {
        self.roles.contains(&Role::Admin)
    }
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
    pub id: GrimId,
    pub friendly_id: String,
    pub name: String,
}

/// A room — belongs to an area, has exits.
#[derive(Component, Debug)]
pub struct Room {
    pub id: GrimId,
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

/// The last player who whispered (`tell`/`whisper`) this character, so `reply`
/// can answer them. Set on delivery; points at a (boot-local) player entity, so
/// a reply fails gracefully if they've since left.
#[derive(Component, Debug)]
pub struct LastWhisperFrom(pub Entity);

/// Marks an entity as an NPC.
#[derive(Component, Debug)]
pub struct Npc;

/// Character is still in-world but the player disconnected (linkdead).
#[derive(Component, Debug)]
pub struct Linkdead;
/// Inserted by the seed world system, read by the client during character creation.
#[derive(Resource, Debug)]
pub struct StartingRoom(pub Entity);

/// Server config: character-name prefixes that may not be used (a new character's
/// canonical name may not begin with any, case-insensitive — see
/// [`crate::validation::is_name_reserved`]). Defaults to
/// [`crate::validation::DEFAULT_RESERVED_NAME_PREFIXES`]; an author overrides or
/// extends it by inserting this resource before adding the scene plugin.
#[derive(Resource, Clone, Debug)]
pub struct ReservedNamePrefixes(pub Vec<String>);

impl Default for ReservedNamePrefixes {
    fn default() -> Self {
        Self(
            crate::validation::DEFAULT_RESERVED_NAME_PREFIXES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn character(roles: Vec<Role>) -> Character {
        Character {
            id: GrimId::new(),
            name: "T".into(),
            account_id: GrimId::new(),
            created_at: chrono::Utc::now(),
            last_room: None,
            roles,
            gender: Gender::Neutral,
            race: String::new(),
            class: String::new(),
            level: 1,
        }
    }

    #[test]
    fn is_admin_reflects_roles() {
        assert!(!character(vec![]).is_admin());
        assert!(character(vec![Role::Admin]).is_admin());
    }

    #[test]
    fn role_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Role::Admin).unwrap(), "\"admin\"");
        assert_eq!(
            serde_json::from_str::<Role>("\"admin\"").unwrap(),
            Role::Admin
        );
    }

    #[test]
    fn character_round_trips_build_fields() {
        let mut ch = character(vec![]);
        ch.gender = Gender::Female;
        ch.race = "dwarf".into();
        ch.class = "mage".into();
        ch.level = 1;
        let json = serde_json::to_string(&ch).unwrap();
        // Gender rides along lowercase, like Role.
        assert!(json.contains("\"gender\":\"female\""));
        let back: Character = serde_json::from_str(&json).unwrap();
        assert_eq!(back.gender, Gender::Female);
        assert_eq!(back.race, "dwarf");
        assert_eq!(back.class, "mage");
        assert_eq!(back.level, 1);
    }

    #[test]
    fn character_json_without_roles_defaults_empty() {
        // Old character files, written before `roles` existed. Ids are Grim IDs
        // (base62 x12) — pre-GrimId UUID files must be migrated first.
        let json = r#"{"id":"aaaaaaaaaaaa","name":"Old","account_id":"bbbbbbbbbbbb","created_at":"2020-01-01T00:00:00Z"}"#;
        let ch: Character = serde_json::from_str(json).unwrap();
        assert!(ch.roles.is_empty());
        assert!(ch.last_room.is_none());
        // New build fields default cleanly for pre-existing JSON.
        assert_eq!(ch.gender, Gender::Neutral);
        assert!(ch.race.is_empty());
        assert!(ch.class.is_empty());
        assert_eq!(ch.level, 1);
    }
}
