//! Completing a character build once the class is picked. Two paths, chosen by
//! account ownership in [`crate::creation::select_class`]:
//!
//! - [`finalize_character`] — a brand-new character: persist + spawn a fresh
//!   entity, link it to the account, advance to the MOTD.
//! - [`backfill_and_enter`] — a legacy character (created before races/classes
//!   existed) routed through the picker once at login: write the chosen build
//!   onto its existing on-disk JSON, then enter the world via the normal
//!   [`world_entry::enter_world_by_name`] path.
//!
//! [`account_owns_named`] is the discriminator between the two.

use bevy::prelude::*;
use chrono::Utc;
use grim_engine_types::components::{
    Account, Character, Client, ClientState, Description, Gender, InRoom, Name as GrimName, Player,
};
use grim_engine_types::GrimId;
use grim_networking::ConnectionOutput;
use grim_persistence::{load_character_by_name, PersistenceConfig};

use crate::formatter;
use crate::params::{SessionRes, WorldEntry};
use crate::session::ConnectedAt;
use crate::world_entry;

/// Whether `client.account` already owns a character named `name`. Resolves the
/// on-disk character's id and tests it against the account's `characters`, so a
/// legacy character routed through the picker (owned, on disk) is distinguished
/// from a brand-new name (not yet owned). Fails closed — an unresolvable account
/// or missing character reads as "not owned" (→ new-character path).
pub(crate) fn account_owns_named(
    accounts: &Query<(Entity, &mut Account)>,
    account: Option<Entity>,
    name: &str,
    persistence: &PersistenceConfig,
) -> bool {
    let Some(account_entity) = account else {
        return false;
    };
    let Ok((_, account)) = accounts.get(account_entity) else {
        return false;
    };
    let Some(existing) = load_character_by_name(persistence, name) else {
        return false;
    };
    account.characters.contains(&existing.id)
}

/// Legacy backfill: write the chosen gender/race/class onto the character's
/// existing on-disk JSON (preserving id, level, account, `last_room`, …), then
/// enter the world via the normal [`world_entry::enter_world_by_name`] path,
/// which reloads the just-updated JSON and spawns it (→ MOTD). One world-entry
/// path, no duplication, and no [`finalize_character`] name-collision reject.
#[allow(clippy::too_many_arguments)]
pub(crate) fn backfill_and_enter(
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
    world: &mut WorldEntry,
) {
    let Some(account_entity) = client.account else {
        return;
    };
    let Ok((_, account)) = accounts.get(account_entity) else {
        return;
    };
    let account_id = account.id;
    // Update the character with the picked build data.
    if let Some(mut character) = load_character_by_name(&res.persistence, &name) {
        character.gender = gender;
        character.race = race;
        character.class = class;
        // Write the JSON under the character's OWN stored name (letters-only, set
        // through validate_character_name at creation), not the raw input, so the
        // path can't be steered outside characters_dir.
        let path = res
            .persistence
            .characters_dir()
            .join(format!("{}.json", character.name));
        let _ = std::fs::create_dir_all(res.persistence.characters_dir());
        if let Ok(json) = serde_json::to_string_pretty(&character) {
            let _ = std::fs::write(path, json);
        }
        // If the character is already resident (linkdead / in-world), its ECS
        // `Character` component is stale — `enter_world_by_name` reuses the
        // existing entity rather than re-reading disk. Refresh the component so a
        // later move/disconnect save can't overwrite the corrected JSON with the
        // old empty race/class.
        if let Some((entity, _, _)) = world
            .characters
            .iter()
            .find(|(_, _, n)| n.0 == character.name)
        {
            commands.entity(entity).insert(character);
        }
    }
    world_entry::enter_world_by_name(
        conn,
        client,
        account_id,
        &name,
        commands,
        &world.characters,
        &world.players,
        &world.linkdead,
        &mut world.histories,
        &world.rooms,
        res.starting.0,
        &res.persistence,
        outputs,
        &mut world.announce_linkdead,
        &mut world.disconnect,
    );
}

/// Persist a fully specified new character (level 1, no XP system) to disk +
/// ECS, link it to the account, and advance the session to the MOTD.
#[allow(clippy::too_many_arguments)]
pub(crate) fn finalize_character(
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
    // Re-check availability at finalize. The name was accepted when the picker
    // started, but another session could have finalized the same name during the
    // gender/race/class steps. Without this, two accounts racing the same name
    // both write `{name}.json` and the later write clobbers the former's
    // character (lost data + a dangling account reference). Send the player back
    // to name entry rather than overwrite.
    if load_character_by_name(&res.persistence, &name).is_some() {
        client.state = ClientState::CreateCharacter;
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(
                conn,
                "That name was just taken.\nEnter a name for your new character: ",
            )
        });
        return;
    }
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
        title: None,
        restrings: std::collections::HashMap::new(),
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
            ConnectedAt(Utc::now()),
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
