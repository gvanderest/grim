//! The standalone new-account email prompt (reached via `new` at the login
//! prompt): email only. Split out of `login.rs` under the module line cap.

use bevy::prelude::*;
use grim_core::components::{Account, Client, ClientState};
use grim_networking::ConnectionOutput;
use grim_text::tr;

use crate::validation::validate_identifier;

/// NewAccountPrompt: email only. Valid + unused advances to the confirm
/// prompt; anything else returns to the login prompt with guidance.
pub(crate) fn new_account_prompt(
    client: &mut Client,
    conn: Entity,
    text: &str,
    accounts: &Query<(Entity, &mut Account)>,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    match validate_identifier(text) {
        Ok(identifier) => {
            if accounts.iter().any(|(_, a)| a.identifier == identifier) {
                client.state = ClientState::LoginPrompt;
                outputs.write(ConnectionOutput {
                    echo: None,
                    ..ConnectionOutput::new(
                        conn,
                        format!("{}{}", tr!("login.email_in_use"), tr!("login.prompt")),
                    )
                });
            } else {
                client.state = ClientState::ConfirmCreate {
                    identifier: identifier.clone(),
                };
                outputs.write(ConnectionOutput {
                    echo: None,
                    ..ConnectionOutput::new(
                        conn,
                        tr!("login.confirm_create", addr = identifier.as_str()),
                    )
                });
            }
        }
        Err(_) => {
            client.state = ClientState::LoginPrompt;
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(
                    conn,
                    format!(
                        "{}{}",
                        tr!("login.new_account_invalid"),
                        tr!("login.prompt")
                    ),
                )
            });
        }
    }
}
