//! The admin-only `sockets` list: one row per live session (connection id,
//! address, session state, character, account), answered session-locally by
//! `handle_ingame` (no engine round-trip).

use bevy::prelude::*;
use grim_actor::{Actor, Character};
use grim_core::components::{Account, ClientState, Name as GrimName};
use grim_networking::Connection;

use crate::formatter::{self, SocketRow};

/// A per-tick snapshot of every session, collected by `handle_ingame_input`
/// before dispatching lines. Passed down so the `sockets` list can render all
/// sessions without a second `Client` query — which would conflict with the
/// `&mut Client` borrow the dispatcher holds (Bevy rejects shared + mutable
/// access to one component in a system).
#[derive(Debug, Clone)]
pub(crate) struct ClientSnapshot {
    pub(crate) client: Entity,
    pub(crate) connection: Entity,
    pub(crate) state: ClientState,
    pub(crate) account: Option<Entity>,
    pub(crate) character: Option<Entity>,
}

/// Short session-state label for the `sockets` list. Exhaustive over
/// [`ClientState`] so a new session state fails to compile until it gets a
/// label here.
fn client_state_label(state: &ClientState) -> &'static str {
    match state {
        ClientState::InGame => "InGame",
        ClientState::LoginPrompt => "Login",
        ClientState::PasswordPrompt { .. } => "Password",
        ClientState::ConfirmCreate { .. } => "Confirm",
        ClientState::CharacterSelect => "Select",
        ClientState::CreateCharacter => "NewChar",
        ClientState::SelectGender { .. } => "Gender",
        ClientState::SelectRace { .. } => "Race",
        ClientState::SelectClass { .. } => "Class",
        ClientState::MotdPrompt => "MOTD",
    }
}

/// The admin-only `sockets` list: one row per live session, sorted by
/// connection id. A session whose `Connection` is gone is skipped (there is
/// no address to show); missing character/account names render as `"-"`.
pub(crate) fn format_sockets(
    snapshot: &[ClientSnapshot],
    connections: &Query<&Connection>,
    characters: &Query<(Entity, &Character, &Actor, &GrimName)>,
    accounts: &Query<&Account>,
) -> String {
    let mut rows: Vec<SocketRow> = snapshot
        .iter()
        .filter_map(|s| {
            let conn = connections.get(s.connection).ok()?;
            let character = s
                .character
                .and_then(|e| characters.get(e).ok())
                .map(|(_, _, _, n)| n.0.clone())
                .unwrap_or_else(|| "-".to_string());
            let account = s
                .account
                .and_then(|e| accounts.get(e).ok())
                .map(|a| a.identifier.clone())
                .unwrap_or_else(|| "-".to_string());
            Some(SocketRow {
                id: conn.id,
                addr: conn.addr.to_string(),
                state: client_state_label(&s.state).to_string(),
                character,
                account,
            })
        })
        .collect();
    rows.sort_by_key(|r| r.id);
    formatter::format_sockets_list(&rows)
}
