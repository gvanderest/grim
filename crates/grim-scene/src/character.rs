//! Character selection & creation: the menu, its stable character listing, and
//! the `CharacterSelect` / `CreateCharacter` / `MotdPrompt` state handlers.

use bevy::prelude::*;
use chrono::Utc;
use grim_engine_types::components::{
    Account, Character, Client, ClientState, Description, Gender, InRoom, Linkdead,
    Name as GrimName, OutputHistory, Player,
};
use grim_engine_types::events::{LinkdeadAnnounce, LoginAnnounce, LookRoom};
use grim_engine_types::validation::{is_name_reserved, validate_character_name};
use grim_engine_types::GrimId;
use grim_networking::{ConnectionOutput, DisconnectRequest};
use grim_persistence::{load_account_characters, PersistenceConfig};
use std::collections::VecDeque;

use crate::formatter::{self, MenuItem};
use crate::params::{RoomResolver, SessionRes};
use crate::world_entry;

/// The three fixed gender options, in menu order: `(value, display, slug)`.
const GENDERS: [(Gender, &str, &str); 3] = [
    (Gender::Male, "Male", "male"),
    (Gender::Female, "Female", "female"),
    (Gender::Neutral, "Neutral", "neutral"),
];

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
    characters: &Query<(Entity, &Character, &GrimName)>,
    persistence: &PersistenceConfig,
) -> Vec<CharEntry> {
    let mut entries: Vec<CharEntry> = Vec::new();
    for (e, ch, name) in characters.iter() {
        if account.characters.contains(&ch.id) {
            entries.push(CharEntry {
                id: ch.id,
                name: name.0.clone(),
                resident: Some(e),
                level: ch.level,
                race: ch.race.clone(),
                class: ch.class.clone(),
            });
        }
    }
    for ch in load_account_characters(persistence, account.id) {
        if !entries.iter().any(|e| e.id == ch.id) {
            entries.push(CharEntry {
                id: ch.id,
                name: ch.name,
                resident: None,
                level: ch.level,
                race: ch.race,
                class: ch.class,
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
    characters: &Query<(Entity, &Character, &GrimName)>,
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
    // Resolve the selection (index or case-insensitive name) to a name.
    let selected: Option<String> = if let Ok(idx) = lower.parse::<usize>() {
        if idx >= 1 {
            entries.get(idx - 1).map(|e| e.name.clone())
        } else {
            None
        }
    } else {
        entries
            .iter()
            .find(|e| e.name.to_lowercase() == lower)
            .map(|e| e.name.clone())
    };

    let Some(name) = selected else {
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

/// CreateCharacter: validate + reserved-name check the typed name, then open
/// the build picker (gender → race → class). The character is NOT persisted
/// here — that happens once a class is chosen (see [`select_class`]).
pub(crate) fn create_character(
    client: &mut Client,
    conn: Entity,
    text: &str,
    res: &SessionRes,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    match validate_character_name(text.trim()) {
        Ok(name) if is_name_reserved(&name, &res.reserved.0) => {
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(
                    conn,
                    "That name is reserved.\nEnter a name for your new character: ",
                )
            });
        }
        Ok(name) => {
            // Name accepted — begin the gender → race → class picker.
            client.state = ClientState::SelectGender { name };
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(conn, gender_menu())
            });
        }
        Err(e) => {
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(
                    conn,
                    format!("Invalid name: {}\nEnter a name for your character: ", e),
                )
            });
        }
    }
}

// ─── Creation picker: gender → race → class ──────────────────────────

/// The gender selection menu (three fixed options).
fn gender_menu() -> String {
    let items: Vec<MenuItem> = GENDERS
        .iter()
        .map(|(_, name, slug)| MenuItem {
            name,
            slug,
            description: "",
        })
        .collect();
    formatter::format_selection_menu("Gender", &items, "Choose a gender: ")
}

/// The race selection menu, built from the [`RaceRegistry`].
fn race_menu(res: &SessionRes) -> String {
    let items: Vec<MenuItem> = res
        .races
        .iter()
        .map(|r| MenuItem {
            name: &r.name,
            slug: &r.slug,
            description: &r.description,
        })
        .collect();
    formatter::format_selection_menu("Race", &items, "Choose a race: ")
}

/// The class selection menu — only the tier-1 (creatable) classes, each with
/// its one-line description.
fn class_menu(res: &SessionRes) -> String {
    let items: Vec<MenuItem> = res
        .classes
        .creatable()
        .map(|c| MenuItem {
            name: &c.name,
            slug: &c.slug,
            description: &c.description,
        })
        .collect();
    formatter::format_selection_menu("Class", &items, "Choose a class: ")
}

/// SelectGender: resolve the pick to a [`Gender`] and advance to race
/// selection, or re-prompt the gender menu on invalid input.
pub(crate) fn select_gender(
    client: &mut Client,
    conn: Entity,
    text: &str,
    name: String,
    res: &SessionRes,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let items: Vec<MenuItem> = GENDERS
        .iter()
        .map(|(_, n, slug)| MenuItem {
            name: n,
            slug,
            description: "",
        })
        .collect();
    match formatter::parse_menu_choice(text, &items) {
        Some(i) => {
            let gender = GENDERS[i].0;
            client.state = ClientState::SelectRace { name, gender };
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(conn, race_menu(res))
            });
        }
        None => reprompt(conn, gender_menu(), outputs),
    }
}

/// SelectRace: resolve the pick to a race slug and advance to class selection,
/// or re-prompt the race menu on invalid input.
pub(crate) fn select_race(
    client: &mut Client,
    conn: Entity,
    text: &str,
    name: String,
    gender: Gender,
    res: &SessionRes,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let items: Vec<MenuItem> = res
        .races
        .iter()
        .map(|r| MenuItem {
            name: &r.name,
            slug: &r.slug,
            description: &r.description,
        })
        .collect();
    match formatter::parse_menu_choice(text, &items) {
        Some(i) => {
            let race = res.races.0[i].slug.clone();
            client.state = ClientState::SelectClass { name, gender, race };
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(conn, class_menu(res))
            });
        }
        None => reprompt(conn, race_menu(res), outputs),
    }
}

/// SelectClass: resolve the pick to a tier-1 class slug, then persist the fully
/// specified character and advance to the MOTD. Re-prompts on invalid input.
#[allow(clippy::too_many_arguments)]
pub(crate) fn select_class(
    client: &mut Client,
    conn: Entity,
    text: &str,
    name: String,
    gender: Gender,
    race: String,
    accounts: &mut Query<(Entity, &mut Account)>,
    res: &SessionRes,
    commands: &mut Commands,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    // Only tier-1 classes are creatable; index into that same filtered list.
    let creatable: Vec<_> = res.classes.creatable().collect();
    let items: Vec<MenuItem> = creatable
        .iter()
        .map(|c| MenuItem {
            name: &c.name,
            slug: &c.slug,
            description: &c.description,
        })
        .collect();
    match formatter::parse_menu_choice(text, &items) {
        Some(i) => {
            let class = creatable[i].slug.clone();
            finalize_character(
                client, conn, name, gender, race, class, accounts, res, commands, outputs,
            );
        }
        None => reprompt(conn, class_menu(res), outputs),
    }
}

/// Persist a fully specified new character (level 1, no XP system) to disk +
/// ECS, link it to the account, and advance the session to the MOTD.
#[allow(clippy::too_many_arguments)]
fn finalize_character(
    client: &mut Client,
    conn: Entity,
    name: String,
    gender: Gender,
    race: String,
    class: String,
    accounts: &mut Query<(Entity, &mut Account)>,
    res: &SessionRes,
    commands: &mut Commands,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let Some(account_entity) = client.account else {
        return;
    };
    let Ok((_, mut account)) = accounts.get_mut(account_entity) else {
        return;
    };
    let char_id = GrimId::new();
    let character = Character {
        id: char_id,
        name: name.clone(),
        account_id: account.id,
        created_at: Utc::now(),
        last_room: None,
        roles: Vec::new(),
        gender,
        race,
        class,
        level: 1,
    };
    // Save character to disk immediately.
    let path = res
        .persistence
        .characters_dir()
        .join(format!("{name}.json"));
    let _ = std::fs::create_dir_all(res.persistence.characters_dir());
    if let Ok(json) = serde_json::to_string_pretty(&character) {
        let _ = std::fs::write(path, json);
    }
    let char_entity = commands
        .spawn((
            character,
            GrimName(name.clone()),
            Description("A new adventurer.".into()),
            Player {
                connection: Some(conn),
            },
            InRoom {
                room: res.starting.0,
            },
        ))
        .id();
    account.characters.push(char_id);
    // Update account JSON with the new character reference.
    let acct_path = res
        .persistence
        .accounts_dir()
        .join(format!("{}.json", account.id));
    let _ = std::fs::create_dir_all(res.persistence.accounts_dir());
    if let Ok(json) = serde_json::to_string_pretty(&*account) {
        let _ = std::fs::write(acct_path, json);
    }
    client.character = Some(char_entity);
    client.state = ClientState::MotdPrompt;
    outputs.write(ConnectionOutput {
        echo: None,
        ..ConnectionOutput::new(conn, formatter::format_motd())
    });
}

/// Re-send a menu without advancing state, prefixed with a rejection notice.
fn reprompt(conn: Entity, menu: String, outputs: &mut MessageWriter<ConnectionOutput>) {
    outputs.write(ConnectionOutput {
        echo: None,
        ..ConnectionOutput::new(conn, format!("Please choose one of the options.\n{menu}"))
    });
}

/// MotdPrompt: acknowledge the MOTD, flip to `InGame`, begin output capture,
/// announce the login, and auto-look the character's room.
pub(crate) fn motd_prompt(
    client: &mut Client,
    characters: &Query<(Entity, &Character, &GrimName)>,
    player_chars: &Query<(Entity, &GrimName, &InRoom, Option<&Character>)>,
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
        .map(|(_, _, n)| n.0.clone())
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
    if let Ok((_, _, ir, _)) = player_chars.get(char_entity) {
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
    characters: &Query<(Entity, &Character, &GrimName)>,
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
                Some(e) if players.get(e).ok().and_then(|p| p.connection).is_some() => " (online)",
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
