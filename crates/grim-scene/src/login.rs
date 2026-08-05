//! Login flow: resolve a character-name-or-email at the prompt, confirm account
//! creation, and handle the password prompt (creating a new account or
//! authenticating an existing one, then routing into the world or the menu).

use bevy::prelude::*;
use chrono::Utc;
use grim_engine_types::components::{
    Account, Character, Client, ClientState, Linkdead, Name as GrimName, OutputHistory, Player,
};
use grim_engine_types::events::LinkdeadAnnounce;
use grim_engine_types::validation::{
    hash_password, normalize_character_name, validate_identifier, validate_password,
    verify_password,
};
use grim_engine_types::GrimId;
use grim_networking::{ConnectionOutput, DisconnectRequest};
use grim_persistence::{load_character_by_name, PersistenceConfig};
use grim_text::tr;

use crate::character;
use crate::creation;
use crate::params::{RoomResolver, SessionRes};
use crate::world_entry;

/// LoginPrompt: try the input as a character name first (resident, linkdead
/// beating online, else disk), falling back to email-identifier validation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn login_prompt(
    client: &mut Client,
    conn: Entity,
    text: &str,
    accounts: &Query<(Entity, &mut Account)>,
    characters: &Query<(Entity, &Character, &GrimName)>,
    linkdead: &Query<&Linkdead>,
    persistence: &PersistenceConfig,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    if text.trim().is_empty() {
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(conn, tr!("login.prompt"))
        });
        return;
    }
    // First, try as a character name. Normalize the raw input to the canonical
    // name once (this is the login-input boundary), then resolve to (account_id,
    // name) WITHOUT needing a resident entity: prefer a resident character
    // (linkdead beats online for the same name), else read from disk.
    let trimmed = text.trim();
    let canonical = normalize_character_name(trimmed);
    let resolved: Option<(GrimId, String)> = (!canonical.is_empty())
        .then(|| {
            characters
                .iter()
                .filter(|(_, _, n)| n.0 == canonical)
                .max_by_key(|(e, _, _)| if linkdead.get(*e).is_ok() { 1 } else { 0 })
                .map(|(_, c, n)| (c.account_id, n.0.clone()))
                .or_else(|| {
                    load_character_by_name(persistence, &canonical).map(|c| (c.account_id, c.name))
                })
        })
        .flatten();
    if let Some((acct_id, name)) = resolved {
        // Only enter the password flow if the owning account is known (accounts
        // are all loaded at startup); otherwise fall through to email validation.
        if let Some((_, account)) = accounts.iter().find(|(_, a)| a.id == acct_id) {
            client.state = ClientState::PasswordPrompt {
                identifier: account.identifier.clone(),
                is_new: false,
                character: Some(name),
            };
            outputs.write(ConnectionOutput {
                echo: Some(false),
                ..ConnectionOutput::new(conn, "Password: ")
            });
            return;
        }
    }
    // Fall back to email validation
    match validate_identifier(text) {
        Ok(identifier) => {
            let exists = accounts.iter().any(|(_, a)| a.identifier == identifier);
            if exists {
                client.state = ClientState::PasswordPrompt {
                    identifier,
                    is_new: false,
                    character: None,
                };
                outputs.write(ConnectionOutput {
                    echo: Some(false),
                    ..ConnectionOutput::new(conn, "Password: ")
                });
            } else {
                client.state = ClientState::ConfirmCreate {
                    identifier: identifier.clone(),
                };
                outputs.write(ConnectionOutput {
                    echo: None,
                    ..ConnectionOutput::new(
                        conn,
                        "Did not find that email address, do you want to create an account? [Y/n] ",
                    )
                });
            }
        }
        Err(e) => {
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(
                    conn,
                    format!(
                        "Invalid identifier: {}\nEnter your character name or email address: ",
                        e
                    ),
                )
            });
        }
    }
}

/// ConfirmCreate: default-Yes prompt to create an account for a new email.
pub(crate) fn confirm_create(
    client: &mut Client,
    conn: Entity,
    text: &str,
    identifier: String,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let first = text.trim().to_lowercase();
    // Empty or "y" / "yes" → create account (default Yes)
    if first.is_empty() || first == "y" || first == "yes" {
        client.state = ClientState::PasswordPrompt {
            identifier,
            is_new: true,
            character: None,
        };
        outputs.write(ConnectionOutput {
            echo: Some(false),
            ..ConnectionOutput::new(conn, "Choose a password: ")
        });
    } else {
        client.state = ClientState::LoginPrompt;
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(conn, tr!("login.prompt"))
        });
    }
}

/// The destructured `PasswordPrompt` state, passed as one argument so the
/// dispatcher stays under clippy's argument limit at the call site.
pub(crate) struct PasswordPromptArgs {
    pub(crate) identifier: String,
    pub(crate) is_new: bool,
    pub(crate) character: Option<String>,
}

/// PasswordPrompt: empty reverts to the login prompt; otherwise create a new
/// account (`is_new`) or authenticate an existing one.
#[allow(clippy::too_many_arguments)]
pub(crate) fn password_prompt(
    args: PasswordPromptArgs,
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
    let PasswordPromptArgs {
        identifier,
        is_new,
        character,
    } = args;
    let auto_select = character;
    if text.trim().is_empty() {
        client.state = ClientState::LoginPrompt;
        outputs.write(ConnectionOutput {
            echo: Some(true),
            ..ConnectionOutput::new(conn, tr!("login.wrong_password"))
        });
        return;
    }
    if is_new {
        create_account(
            client,
            client_entity,
            conn,
            text,
            &identifier,
            &res.persistence,
            characters,
            accounts,
            players,
            linkdead,
            commands,
            outputs,
        );
    } else {
        authenticate(
            client,
            client_entity,
            conn,
            text,
            &identifier,
            auto_select,
            accounts,
            characters,
            players,
            linkdead,
            histories,
            rooms,
            res,
            commands,
            outputs,
            announce_linkdead,
            disconnect,
        );
    }
}

/// Validate the chosen password, persist a new account to disk + ECS, and show
/// the (empty) character menu.
#[allow(clippy::too_many_arguments)]
fn create_account(
    client: &mut Client,
    client_entity: Entity,
    conn: Entity,
    text: &str,
    identifier: &str,
    persistence: &PersistenceConfig,
    characters: &Query<(Entity, &Character, &GrimName)>,
    accounts: &Query<(Entity, &mut Account)>,
    players: &Query<&Player>,
    linkdead: &Query<&Linkdead>,
    commands: &mut Commands,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    match validate_password(text.trim()) {
        Ok(()) => {
            let account = Account {
                id: GrimId::new(),
                identifier: identifier.to_string(),
                password_hash: hash_password(text.trim()),
                characters: vec![],
                created_at: Utc::now(),
            };
            // Save to disk immediately
            let path = persistence
                .accounts_dir()
                .join(format!("{}.json", account.id));
            let _ = std::fs::create_dir_all(persistence.accounts_dir());
            if let Ok(json) = serde_json::to_string_pretty(&account) {
                let _ = std::fs::write(path, json);
            }
            let account_entity = commands.spawn(account).id();
            client.account = Some(account_entity);
            client.state = ClientState::CharacterSelect;
            // Restore echo before showing menu
            outputs.write(ConnectionOutput {
                echo: Some(true),
                ..ConnectionOutput::new(conn, "")
            });
            character::show_character_menu(
                client_entity,
                client,
                characters,
                accounts,
                outputs,
                linkdead,
                players,
                persistence,
            );
        }
        Err(e) => {
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(
                    conn,
                    format!("Invalid password: {}\nChoose a password: ", e),
                )
            });
        }
    }
}

/// Verify the password for an existing account, then enter the world directly
/// (login-by-name auto-select) or show the character menu.
#[allow(clippy::too_many_arguments)]
fn authenticate(
    client: &mut Client,
    client_entity: Entity,
    conn: Entity,
    text: &str,
    identifier: &str,
    auto_select: Option<String>,
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
    let account_found = accounts
        .iter()
        .find(|(_, a)| a.identifier == *identifier)
        .map(|(e, a)| (e, a.id));
    match account_found {
        Some((account_entity, account_id)) => {
            let ok = accounts
                .get(account_entity)
                .map(|(_, a)| verify_password(text.trim(), &a.password_hash))
                .unwrap_or(false);
            if ok {
                client.account = Some(account_entity);
                if let Some(name) = auto_select {
                    // A legacy character (no race/class yet) is routed through the
                    // creation picker once before entering — same as the
                    // menu-selection path (character_select). Otherwise: straight
                    // into the world (reconnect / takeover / spawn).
                    let legacy = load_character_by_name(&res.persistence, &name)
                        .map(|c| {
                            c.account_id == account_id && c.race.is_empty() && c.class.is_empty()
                        })
                        .unwrap_or(false);
                    if legacy {
                        creation::start_gender_pick(client, conn, name, outputs);
                    } else {
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
                } else {
                    client.state = ClientState::CharacterSelect;
                    character::show_character_menu(
                        client_entity,
                        client,
                        characters,
                        accounts,
                        outputs,
                        linkdead,
                        players,
                        &res.persistence,
                    );
                }
            } else {
                outputs.write(ConnectionOutput {
                    echo: None,
                    ..ConnectionOutput::new(conn, "Invalid password.\nPassword: ")
                });
            }
        }
        None => {
            client.state = ClientState::LoginPrompt;
            outputs.write(ConnectionOutput {
                echo: Some(true),
                ..ConnectionOutput::new(conn, "Account not found.\nEnter your email address: ")
            });
        }
    }
}
