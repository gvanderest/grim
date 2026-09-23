//! Login flow: resolve a character-name-or-email at the prompt (`help`/`new`
//! keywords, per-case guidance), confirm account creation for a new email,
//! and handle the password prompt (creating a new account or authenticating
//! an existing one, then routing into the world or the menu).

use bevy::prelude::*;
use grim_actor::{Actor, Character, Linkdead};
use grim_core::components::{Account, Client, ClientState, Name as GrimName};
use grim_core::GrimId;
use grim_networking::ConnectionOutput;
use grim_persistence::{load_character_by_name, PersistenceConfig};
use grim_text::tr;

use crate::validation::{normalize_character_name, validate_character_name, validate_identifier};

pub(crate) use crate::new_account::new_account_prompt;
pub(crate) use crate::password::{password_prompt, PasswordPromptArgs};

/// LoginPrompt: `help`/`new` keywords first, then a character name (resident,
/// linkdead beating online, else disk), falling back to email-identifier
/// validation. Unknown names/emails get guidance, not a bare error.
#[allow(clippy::too_many_arguments)]
pub(crate) fn login_prompt(
    client: &mut Client,
    conn: Entity,
    text: &str,
    accounts: &Query<(Entity, &mut Account)>,
    characters: &Query<(Entity, &Character, &Actor, &GrimName)>,
    linkdead: &Query<&Linkdead>,
    persistence: &PersistenceConfig,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(conn, tr!("login.prompt"))
        });
        return;
    }
    if handle_login_keyword(client, conn, trimmed, outputs) {
        return;
    }
    if try_login_by_name(
        client,
        conn,
        trimmed,
        accounts,
        characters,
        linkdead,
        persistence,
        outputs,
    ) {
        return;
    }
    resolve_login_email(client, conn, text, accounts, outputs);
}

/// `help`/`new` keywords at the login prompt. Returns true when handled.
fn handle_login_keyword(
    client: &mut Client,
    conn: Entity,
    trimmed: &str,
    outputs: &mut MessageWriter<ConnectionOutput>,
) -> bool {
    let keyword = trimmed.to_lowercase();
    if keyword == "help" {
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(
                conn,
                format!(
                    "{}\n{}",
                    include_str!("../../../assets/login-help.txt"),
                    tr!("login.prompt")
                ),
            )
        });
        return true;
    }
    if keyword == "new" {
        client.state = ClientState::NewAccountPrompt;
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(conn, tr!("login.new_account_prompt"))
        });
        return true;
    }
    false
}

/// Try the input as a character name: enter the password flow when it
/// resolves, report not-found guidance when it is name-shaped but unknown.
/// Returns true when handled (the email phase is skipped).
#[allow(clippy::too_many_arguments)]
fn try_login_by_name(
    client: &mut Client,
    conn: Entity,
    trimmed: &str,
    accounts: &Query<(Entity, &mut Account)>,
    characters: &Query<(Entity, &Character, &Actor, &GrimName)>,
    linkdead: &Query<&Linkdead>,
    persistence: &PersistenceConfig,
    outputs: &mut MessageWriter<ConnectionOutput>,
) -> bool {
    // A valid character name resolves to (account_id, name) WITHOUT needing a
    // resident entity: prefer a resident character (linkdead beats online),
    // else read from disk.
    let mut resolved: Option<(GrimId, String)> = None;
    if validate_character_name(trimmed).is_ok() {
        let canonical = normalize_character_name(trimmed);
        resolved = characters
            .iter()
            .filter(|(_, _, _, n)| n.0 == canonical)
            .max_by_key(|(e, _, _, _)| i32::from(linkdead.get(*e).is_ok()))
            .map(|(_, c, _, n)| (c.account_id, n.0.clone()))
            .or_else(|| {
                load_character_by_name(persistence, &canonical).map(|c| (c.account_id, c.name))
            });
    }
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
                ..ConnectionOutput::new(conn, tr!("login.password_prompt"))
            });
            return true;
        }
    }
    // The input was a valid character name but matched nothing: guidance, not
    // a bare error. A valid email shape that matched nothing is handled by the
    // email branch below.
    if validate_character_name(trimmed).is_ok() && validate_identifier(trimmed).is_err() {
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(
                conn,
                format!(
                    "{}{}",
                    tr!("login.character_not_found"),
                    tr!("login.prompt")
                ),
            )
        });
        return true;
    }
    false
}

/// Fall back to email validation: known addresses enter the password flow,
/// unknown ones are offered account creation, anything else gets guidance.
fn resolve_login_email(
    client: &mut Client,
    conn: Entity,
    text: &str,
    accounts: &Query<(Entity, &mut Account)>,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
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
                    ..ConnectionOutput::new(conn, tr!("login.password_prompt"))
                });
            } else {
                client.state = ClientState::ConfirmCreate {
                    identifier: identifier.clone(),
                };
                outputs.write(ConnectionOutput {
                    echo: None,
                    ..ConnectionOutput::new(
                        conn,
                        format!(
                            "{}{}",
                            tr!("login.account_not_found"),
                            tr!("login.confirm_create", addr = identifier.as_str())
                        ),
                    )
                });
            }
        }
        Err(_) => {
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(
                    conn,
                    format!("{}{}", tr!("login.invalid_input"), tr!("login.prompt")),
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
            ..ConnectionOutput::new(conn, tr!("login.choose_password"))
        });
    } else {
        client.state = ClientState::LoginPrompt;
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(conn, tr!("login.prompt"))
        });
    }
}
