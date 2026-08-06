//! The [`Character`] being and the [`Role`] privileges it can hold.
//!
//! A character belongs to an account and can be in-world. It is persisted to
//! `data/characters/<name>.json`, so most fields carry `#[serde(default)]` to
//! keep older character JSON loading cleanly.

use std::collections::HashMap;

use bevy::prelude::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use grim_engine_types::components::RoomLocation;
// `Gender` stays in `grim-engine-types` (the session `ClientState` references it
// during creation), so the actor's `Character` points *up* at it — never the
// reverse.
use grim_engine_types::character::Gender;
use grim_engine_types::id::GrimId;

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
    /// Race slug (e.g. `"human"`), keyed into `grim_world::RaceRegistry`. Empty
    /// on old JSON (and never picked); resolve leniently.
    #[serde(default)]
    pub race: String,
    /// Class slug (e.g. `"warrior"`), keyed into `grim_world::ClassRegistry`.
    /// Holds a single tier-1 slug for now; a future reroll swaps it to the
    /// tier-2 `evolves_to` (see `docs/adr/0002-character-class-tiers.md`).
    #[serde(default)]
    pub class: String,
    /// Character level. New characters start at 1; there is no XP system yet,
    /// so this is just a stored number. `#[serde(default = ...)]` gives old
    /// JSON level 1 rather than 0.
    #[serde(default = "default_level")]
    pub level: u32,
    /// An optional self-set descriptor shown after the name in the WHO list.
    /// Capped at 60 chars by the `title` command. `#[serde(default)]` keeps old
    /// JSON (written before titles existed) loading cleanly.
    #[serde(default)]
    pub title: Option<String>,
    /// Persisted per-character display overrides, keyed by a recognized name.
    /// The WHO renderer honours `who_level`, `who_gender`, `who_race`,
    /// `who_class`, `who_guild` (each overrides one stat column) and `who`
    /// (replaces the whole stat block). No in-game setter yet — edited on disk.
    /// `#[serde(default)]` keeps old JSON loading cleanly.
    #[serde(default)]
    pub restrings: HashMap<String, String>,
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
            title: None,
            restrings: HashMap::new(),
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
    fn character_round_trips_title_and_restrings() {
        let mut ch = character(vec![]);
        ch.title = Some("the Bold".into());
        ch.restrings.insert("who_class".into(), "God".into());
        let json = serde_json::to_string(&ch).unwrap();
        let back: Character = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title.as_deref(), Some("the Bold"));
        assert_eq!(
            back.restrings.get("who_class").map(String::as_str),
            Some("God")
        );
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
        // Titles/restrings default cleanly for pre-existing JSON.
        assert!(ch.title.is_none());
        assert!(ch.restrings.is_empty());
    }
}
