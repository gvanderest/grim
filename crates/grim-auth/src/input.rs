//! The pre-game input dispatcher: routes each input line to the handler for the
//! session's current pre-game [`ClientState`]. This is one half of the routing
//! split — the in-game half lives in `grim_scene`. Any client already `InGame`
//! is skipped here (scene owns it); every pre-game state is handled here.
//!
//! ## Intra-tick transition guard
//! A single line can advance a client from a pre-game state to `InGame` (the
//! MOTD ENTER, or a login-by-name that reconnects a linkdead character). That
//! same [`ConnectionInput`] must NOT then be re-processed as an in-game command
//! by the scene in-game system in the same tick. This system runs BEFORE the
//! scene in-game system (ordered via [`grim_scene::SceneSystems::InGameInput`])
//! and records every connection it advances to `InGame` in
//! [`grim_scene::JustEnteredWorld`]; the scene system skips the triggering line
//! for those connections. The set is cleared at the top of this system each
//! tick, so it only ever holds this tick's transitions.

use bevy::prelude::*;
use grim_core::components::{Account, Client, ClientState};
use grim_core::events::{LoginAnnounce, LookRoom};
use grim_networking::{ConnectionInput, ConnectionOutput};
use grim_scene::JustEnteredWorld;

use crate::character_select as character;
use crate::creation;
use crate::login;
use crate::params::{PlayerChars, SessionRes, WorldEntry};

// A flat dispatch: one match arm per pre-game ClientState, each delegating to
// its handler. Long by nature (a state table) — waived, not split.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn handle_pregame_input(
    mut inputs: MessageReader<ConnectionInput>,
    mut clients: Query<(Entity, &mut Client)>,
    mut accounts: Query<(Entity, &mut Account)>,
    player_chars: PlayerChars,
    res: SessionRes,
    mut just_entered: ResMut<JustEnteredWorld>,
    mut commands: Commands,
    mut outputs: MessageWriter<ConnectionOutput>,
    mut look_room: MessageWriter<LookRoom>,
    mut announce_login: MessageWriter<LoginAnnounce>,
    mut world: WorldEntry,
) {
    // Only ever hold this tick's pre-game → InGame transitions (see module doc).
    just_entered.0.clear();
    for ev in inputs.read() {
        let Some((client_entity, mut client)) = clients
            .iter_mut()
            .find(|(_, c)| c.connection == ev.connection)
        else {
            continue;
        };
        let text = ev.text.as_str();
        let conn = client.connection;

        // Match on a clone of the state so each handler can freely mutate the
        // borrowed `Client` (including its `state`) without a borrow conflict.
        match client.state.clone() {
            // In-game input is the scene system's job; skip it here.
            ClientState::InGame => continue,
            ClientState::LoginPrompt => login::login_prompt(
                &mut client,
                conn,
                text,
                &accounts,
                &world.characters,
                &world.linkdead,
                &res.persistence,
                &mut outputs,
            ),
            ClientState::ConfirmCreate { identifier } => {
                login::confirm_create(&mut client, conn, text, identifier, &mut outputs);
            }
            ClientState::PasswordPrompt {
                identifier,
                is_new,
                character,
            } => login::password_prompt(
                login::PasswordPromptArgs {
                    identifier,
                    is_new,
                    character,
                },
                client_entity,
                &mut client,
                conn,
                text,
                &accounts,
                &world.characters,
                &world.players,
                &world.linkdead,
                &mut world.histories,
                &world.rooms,
                &res,
                &mut commands,
                &mut outputs,
                &mut world.announce_linkdead,
                &mut world.disconnect,
            ),
            ClientState::CharacterSelect => character::character_select(
                client_entity,
                &mut client,
                conn,
                text,
                &accounts,
                &world.characters,
                &world.players,
                &world.linkdead,
                &mut world.histories,
                &world.rooms,
                &res,
                &mut commands,
                &mut outputs,
                &mut world.announce_linkdead,
                &mut world.disconnect,
            ),
            ClientState::CreateCharacter => {
                creation::create_character(&mut client, conn, text, &res, &mut outputs);
            }
            ClientState::SelectGender { name } => {
                creation::select_gender(&mut client, conn, text, name, &res, &mut outputs);
            }
            ClientState::SelectRace { name, gender } => {
                creation::select_race(&mut client, conn, text, name, gender, &res, &mut outputs);
            }
            ClientState::SelectClass { name, gender, race } => creation::select_class(
                &mut client,
                conn,
                text,
                name,
                gender,
                race,
                &mut accounts,
                &res,
                &mut commands,
                &mut outputs,
                &mut world,
            ),
            ClientState::MotdPrompt => character::motd_prompt(
                &mut client,
                &world.characters,
                &player_chars,
                &mut commands,
                &mut announce_login,
                &mut look_room,
            ),
        }

        // If this line advanced the session into the world, record its
        // connection so the scene in-game system skips the same line this tick.
        if client.state == ClientState::InGame {
            just_entered.0.insert(ev.connection);
        }
    }
}
