//! Client input entry points: the connection-established greeter and the
//! per-connection input dispatcher that routes each line to the handler for the
//! session's current [`ClientState`].

use bevy::prelude::*;
use grim_engine_types::components::{
    Account, Character, Client, ClientState, InRoom, Linkdead, Name as GrimName, OutputHistory,
    Player,
};
use grim_engine_types::events::{LinkdeadAnnounce, LoginAnnounce, LookRoom};
use grim_networking::{
    ConnectionEstablished, ConnectionInput, ConnectionOutput, DisconnectRequest,
};
use grim_text::tr;

use crate::character;
use crate::command;
use crate::creation;
use crate::login;
use crate::params::{RoomResolver, SessionRes};

pub(crate) fn handle_connection_established(
    mut established: MessageReader<ConnectionEstablished>,
    mut commands: Commands,
    mut outputs: MessageWriter<ConnectionOutput>,
) {
    for ev in established.read() {
        commands.spawn(Client::new(ev.connection));
        let banner = grim_color::ansi(include_str!("../../../assets/login-banner.txt"));
        let text = format!("{}\n\n{}", banner, tr!("login.prompt"));
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(ev.connection, text)
        });
    }
}

// A flat dispatch: one match arm per ClientState, each delegating to its handler.
// Long by nature (a state table, like parser::build_registry) — waived, not split.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn handle_client_input(
    mut inputs: MessageReader<ConnectionInput>,
    mut clients: Query<(Entity, &mut Client)>,
    mut accounts: Query<(Entity, &mut Account)>,
    characters: Query<(Entity, &Character, &GrimName)>,
    player_chars: Query<(Entity, &GrimName, &InRoom, Option<&Character>)>,
    players: Query<&Player>,
    rooms: RoomResolver,
    res: SessionRes,
    mut commands: Commands,
    mut outputs: MessageWriter<ConnectionOutput>,
    mut look_room: MessageWriter<LookRoom>,
    mut announce_login: MessageWriter<LoginAnnounce>,
    mut announce_linkdead: MessageWriter<LinkdeadAnnounce>,
    linkdead: Query<&Linkdead>,
    mut histories: Query<&mut OutputHistory>,
    mut disconnect: MessageWriter<DisconnectRequest>,
) {
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
            ClientState::LoginPrompt => login::login_prompt(
                &mut client,
                conn,
                text,
                &accounts,
                &characters,
                &linkdead,
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
                &characters,
                &players,
                &linkdead,
                &mut histories,
                &rooms,
                &res,
                &mut commands,
                &mut outputs,
                &mut announce_linkdead,
                &mut disconnect,
            ),
            ClientState::CharacterSelect => character::character_select(
                client_entity,
                &mut client,
                conn,
                text,
                &accounts,
                &characters,
                &players,
                &linkdead,
                &mut histories,
                &rooms,
                &res,
                &mut commands,
                &mut outputs,
                &mut announce_linkdead,
                &mut disconnect,
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
            ),
            ClientState::MotdPrompt => character::motd_prompt(
                &mut client,
                &characters,
                &player_chars,
                &mut commands,
                &mut announce_login,
                &mut look_room,
            ),
            ClientState::InGame => command::handle_ingame(
                &mut client,
                conn,
                text,
                &characters,
                &player_chars,
                &linkdead,
                &rooms,
                &res,
                &mut outputs,
            ),
        }
    }
}
