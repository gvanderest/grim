//! In-game input routing: the per-connection dispatcher that parses each line
//! for a session in [`ClientState::InGame`] into an in-game command.
//!
//! This is one half of the routing split — the pre-game half (login / creation
//! / character-select / MOTD) lives in the auth crate. This system handles ONLY
//! `InGame` clients; every pre-game state is skipped here.
//!
//! ## Intra-tick transition guard
//! A single pre-game line can advance a client to `InGame` in the same tick
//! (the MOTD ENTER, or a login-by-name reconnect). The pre-game (auth) system
//! runs first, consumes that line, and records the connection in
//! [`JustEnteredWorld`]. This system skips any connection in that set so the
//! triggering line is not re-dispatched as an in-game command. Ordering is
//! enforced by [`crate::SceneSystems::InGameInput`] (auth runs `.before` it).

use bevy::prelude::*;
use grim_actor::{Actor, Character, Linkdead};
use grim_core::components::{Client, ClientState, Name as GrimName};
use grim_networking::{ConnectionInput, ConnectionOutput};

use crate::command;
use crate::params::{PlayerChars, RoomResolver, SessionRes};
use crate::session::JustEnteredWorld;

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_ingame_input(
    mut inputs: MessageReader<ConnectionInput>,
    mut clients: Query<(Entity, &mut Client)>,
    characters: Query<(Entity, &Character, &Actor, &GrimName)>,
    player_chars: PlayerChars,
    linkdead: Query<&Linkdead>,
    rooms: RoomResolver,
    res: SessionRes,
    just_entered: Res<JustEnteredWorld>,
    mut outputs: MessageWriter<ConnectionOutput>,
) {
    for ev in inputs.read() {
        let Some((_, mut client)) = clients
            .iter_mut()
            .find(|(_, c)| c.connection == ev.connection)
        else {
            continue;
        };
        // Pre-game states are the auth system's job.
        if client.state != ClientState::InGame {
            continue;
        }
        // This line drove the session into the world this tick; the pre-game
        // system already consumed it. Don't re-dispatch it as a command.
        if just_entered.0.contains(&ev.connection) {
            continue;
        }
        let conn = client.connection;
        command::handle_ingame(
            &mut client,
            conn,
            ev.text.as_str(),
            &characters,
            &player_chars,
            &linkdead,
            &rooms,
            &res,
            &mut outputs,
        );
    }
}
