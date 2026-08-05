//! Character creation: name entry and the gender → race → class picker, ending
//! in a persisted level-1 character. The `CharacterSelect` / menu side lives in
//! [`crate::character`]; this module owns the `CreateCharacter` / `SelectGender`
//! / `SelectRace` / `SelectClass` state handlers.

use bevy::prelude::*;
use chrono::Utc;
use grim_engine_types::components::{
    Account, Character, Client, ClientState, Description, Gender, InRoom, Name as GrimName, Player,
};
use grim_engine_types::validation::{is_name_reserved, validate_character_name};
use grim_engine_types::GrimId;
use grim_networking::ConnectionOutput;
use grim_persistence::load_character_by_name;

use crate::formatter::{self, MenuItem};
use crate::params::SessionRes;

/// The three fixed genders, paired with display name + slug for the menu.
const GENDERS: [(Gender, &str, &str); 3] = [
    (Gender::Male, "Male", "male"),
    (Gender::Female, "Female", "female"),
    (Gender::Neutral, "Neutral", "neutral"),
];

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
        Ok(name) if load_character_by_name(&res.persistence, &name).is_some() => {
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(
                    conn,
                    "That name is already taken.\nEnter a name for your new character: ",
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

/// The race selection menu, built from the [`grim_engine_types::components::RaceRegistry`].
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
