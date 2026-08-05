//! Character selection & creation: the menu, its stable character listing, and
//! the `CharacterSelect` / `CreateCharacter` / `MotdPrompt` state handlers.

use bevy::prelude::*;
use chrono::Utc;
use grim_engine_types::components::{
    Account, Character, Client, ClientState, Description, InRoom, Linkdead, Name as GrimName,
    OutputHistory, Player,
};
use grim_engine_types::events::{LinkdeadAnnounce, LoginAnnounce, LookRoom};
use grim_engine_types::validation::{is_name_reserved, validate_character_name};
use grim_engine_types::GrimId;
use grim_networking::{ConnectionOutput, DisconnectRequest};
use grim_persistence::{load_account_characters, PersistenceConfig};
use std::collections::VecDeque;

use crate::formatter;
use crate::params::{RoomResolver, SessionRes};
use crate::world_entry;

/// One selectable character for the account menu / selection. A logged-out
/// character has no ECS entity, so `resident` is `None` and it lives only on
/// disk; a resident entity (in-world / linkdead) carries `Some(entity)`.
pub(crate) struct CharEntry {
    pub(crate) id: GrimId,
    pub(crate) name: String,
    pub(crate) resident: Option<Entity>,
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
            });
        }
    }
    for ch in load_account_characters(persistence, account.id) {
        if !entries.iter().any(|e| e.id == ch.id) {
            entries.push(CharEntry {
                id: ch.id,
                name: ch.name,
                resident: None,
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

/// CreateCharacter: validate + reserved-name check, then persist a new
/// character to disk + ECS, link it to the account, and advance to the MOTD.
pub(crate) fn create_character(
    client: &mut Client,
    conn: Entity,
    text: &str,
    accounts: &mut Query<(Entity, &mut Account)>,
    res: &SessionRes,
    commands: &mut Commands,
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
            };
            // Save character to disk immediately
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
            // Update account JSON with new character reference
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
                "{}. {} - 1 Human Adventurer{}\n",
                idx, entry.name, suffix
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
