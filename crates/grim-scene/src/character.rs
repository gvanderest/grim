//! Character selection & creation: the menu, its stable character listing, and
//! the `CharacterSelect` / `CreateCharacter` / `MotdPrompt` state handlers.

use bevy::prelude::*;
use grim_actor::{Actor, Character, Linkdead, OutputHistory, Player};
use grim_core::components::{Account, Client, ClientState, Name as GrimName};
use grim_core::events::{LinkdeadAnnounce, LoginAnnounce, LookRoom};
use grim_core::GrimId;
use grim_networking::{ConnectionOutput, DisconnectRequest};
use grim_persistence::{load_account_characters, PersistenceConfig};
use std::collections::VecDeque;

use crate::creation;
use crate::params::{PlayerChars, RoomResolver, SessionRes};
use crate::world_entry;

/// One selectable character for the account menu / selection. A logged-out
/// character has no ECS entity, so `resident` is `None` and it lives only on
/// disk; a resident entity (in-world / linkdead) carries `Some(entity)`.
pub(crate) struct CharEntry {
    pub(crate) id: GrimId,
    pub(crate) name: String,
    pub(crate) resident: Option<Entity>,
    /// Stored level (always 1 today — no XP system yet).
    pub(crate) level: u32,
    /// Stored race slug, empty for characters created before races existed.
    pub(crate) race: String,
    /// Stored class slug, empty for characters created before classes existed.
    pub(crate) class: String,
}

/// A short menu descriptor for a character: `Level N Race Class`, e.g.
/// `Level 1 Human Warrior`. Legacy characters with no race/class fall back to
/// `Level N Adventurer`. Slugs are title-cased for display (`half-elf` →
/// `Half-Elf`) so no registry lookup is needed here.
fn char_descriptor(entry: &CharEntry) -> String {
    let mut parts = format!("Level {}", entry.level);
    if entry.race.is_empty() && entry.class.is_empty() {
        parts.push_str(" Adventurer");
        return parts;
    }
    if !entry.race.is_empty() {
        parts.push(' ');
        parts.push_str(&title_case_slug(&entry.race));
    }
    if !entry.class.is_empty() {
        parts.push(' ');
        parts.push_str(&title_case_slug(&entry.class));
    }
    parts
}

/// Title-case a hyphenated slug for display: `half-orc` → `Half-Orc`.
fn title_case_slug(slug: &str) -> String {
    slug.split('-')
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

/// The account's characters as a stable, deduped, name-sorted list: resident
/// (owned) entities UNION the account's on-disk characters, deduped by id with
/// the resident copy winning. Used by BOTH `show_character_menu` (display) and
/// `CharacterSelect` (index/name resolution) so numbering always agrees.
pub(crate) fn account_character_list(
    account: &Account,
    characters: &Query<(Entity, &Character, &Actor, &GrimName)>,
    persistence: &PersistenceConfig,
) -> Vec<CharEntry> {
    let mut entries: Vec<CharEntry> = Vec::new();
    for (e, ch, actor, name) in characters.iter() {
        if account.characters.contains(&ch.id) {
            entries.push(CharEntry {
                id: ch.id,
                name: name.0.clone(),
                resident: Some(e),
                // Level/race live on the shared `Actor` base now.
                level: actor.level,
                race: actor.race.clone(),
                class: ch.class.clone(),
            });
        }
    }
    for stored in load_account_characters(persistence, account.id) {
        if !entries.iter().any(|e| e.id == stored.id) {
            entries.push(CharEntry {
                id: stored.id,
                name: stored.name,
                resident: None,
                level: stored.level,
                race: stored.race,
                class: stored.class,
            });
        }
    }
    entries.sort_by_key(|e| e.name.to_lowercase());
    entries
}

/// CharacterSelect: `create`/`c` opens creation; otherwise resolve the input
/// (index or case-insensitive name) against the account's list and enter the
/// world, re-showing the menu on an unrecognised selection.
#[allow(clippy::too_many_arguments)]
pub(crate) fn character_select(
    client_entity: Entity,
    client: &mut Client,
    conn: Entity,
    text: &str,
    accounts: &Query<(Entity, &mut Account)>,
    characters: &Query<(Entity, &Character, &Actor, &GrimName)>,
    players: &Query<&Player>,
    linkdead: &Query<&Linkdead>,
    histories: &mut Query<&mut OutputHistory>,
    rooms: &RoomResolver,
    res: &SessionRes,
    commands: &mut Commands,
    outputs: &mut MessageWriter<ConnectionOutput>,
    announce_linkdead: &mut MessageWriter<LinkdeadAnnounce>,
    disconnect: &mut MessageWriter<DisconnectRequest>,
) {
    let text = text.trim();
    let lower = text.to_lowercase();
    if lower == "create" || lower == "c" {
        client.state = ClientState::CreateCharacter;
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(conn, "Enter a name for your new character: ")
        });
        return;
    }

    let Some(account_entity) = client.account else {
        return;
    };
    let Ok((_, account)) = accounts.get(account_entity) else {
        return;
    };
    let account_id = account.id;
    // Resident (owned) UNION disk, deduped by id, stable order.
    let entries = account_character_list(account, characters, &res.persistence);
    // Resolve the selection (index or case-insensitive name) to a list entry.
    let selected: Option<&CharEntry> = if let Ok(idx) = lower.parse::<usize>() {
        if idx >= 1 {
            entries.get(idx - 1)
        } else {
            None
        }
    } else {
        entries.iter().find(|e| e.name.to_lowercase() == lower)
    };

    let Some(entry) = selected else {
        show_character_menu(
            client_entity,
            client,
            characters,
            accounts,
            outputs,
            linkdead,
            players,
            &res.persistence,
        );
        return;
    };
    let name = entry.name.clone();

    // Legacy character: created before races/classes existed, so both slugs are
    // empty on disk. Route it through the gender → race → class picker ONCE
    // (the `select_class` backfill path then persists the build and enters the
    // world). A normal character enters directly, exactly as before.
    if entry.race.is_empty() && entry.class.is_empty() {
        creation::start_gender_pick(client, conn, name, outputs);
        return;
    }

    world_entry::enter_world_by_name(
        conn,
        client,
        account_id,
        &name,
        commands,
        characters,
        players,
        linkdead,
        histories,
        rooms,
        res.starting.0,
        &res.persistence,
        outputs,
        announce_linkdead,
        disconnect,
    );
}

/// MotdPrompt: acknowledge the MOTD, flip to `InGame`, begin output capture,
/// announce the login, and auto-look the character's room.
pub(crate) fn motd_prompt(
    client: &mut Client,
    characters: &Query<(Entity, &Character, &Actor, &GrimName)>,
    player_chars: &PlayerChars,
    commands: &mut Commands,
    announce_login: &mut MessageWriter<LoginAnnounce>,
    look_room: &mut MessageWriter<LookRoom>,
) {
    let conn = client.connection;
    let Some(char_entity) = client.character else {
        return;
    };
    let char_name = characters
        .get(char_entity)
        .map(|(_, _, _, n)| n.0.clone())
        .unwrap_or_else(|_| "Someone".into());
    info!("Character '{}' entered the world", char_name);
    client.state = ClientState::InGame;
    client.input_queue = VecDeque::new();
    client.command_cooldown = Timer::from_seconds(0.5, TimerMode::Once);
    // Start output capture now that the character is in the world
    commands.entity(conn).insert(OutputHistory::with_max(100));
    announce_login.write(LoginAnnounce { name: char_name });
    let Some(char_entity) = client.character else {
        return;
    };
    if let Ok((_, _, ir, _, _, _)) = player_chars.get(char_entity) {
        look_room.write(LookRoom {
            target: char_entity,
            room: ir.room,
        });
    }
}

/// Build and send the character selection menu.
#[allow(clippy::too_many_arguments)]
pub(crate) fn show_character_menu(
    _client_entity: Entity,
    client: &Client,
    characters: &Query<(Entity, &Character, &Actor, &GrimName)>,
    accounts: &Query<(Entity, &mut Account)>,
    outputs: &mut MessageWriter<ConnectionOutput>,
    linkdead: &Query<&Linkdead>,
    players: &Query<&Player>,
    persistence: &PersistenceConfig,
) {
    let conn = client.connection;
    let Some(account_entity) = client.account else {
        return;
    };
    // Account entity may not exist yet if just created via commands.spawn
    // (deferred execution). Handle gracefully by showing empty menu.
    let welcome = match accounts.get(account_entity) {
        Ok((_, account)) => format!("Welcome back, {}!\n", account.identifier),
        Err(_) => "Welcome!\n".into(),
    };
    let mut menu = format!("{}\n[ Characters ]\n", welcome);
    let mut idx = 1;
    // Fail closed: if the account entity is not resolvable (e.g. spawned this
    // frame via commands.spawn and not yet flushed, which is exactly the case
    // for an account created moments ago), show no characters.
    if let Ok((_, account)) = accounts.get(account_entity) {
        for entry in account_character_list(account, characters, persistence) {
            let suffix = match entry.resident {
                Some(e) if linkdead.get(e).is_ok() => " (linkdead)",
                Some(e) if players.get(e).is_ok() => " (online)",
                _ => "",
            };
            menu.push_str(&format!(
                "{}. {} - {}{}\n",
                idx,
                entry.name,
                char_descriptor(&entry),
                suffix
            ));
            idx += 1;
        }
    }
    if idx == 1 {
        menu.push_str("You have no characters created yet.\n");
    }
    menu.push_str("\nc: Create a new character\n\nWhat would you like to do? ");
    outputs.write(ConnectionOutput::new(conn, menu));
}
