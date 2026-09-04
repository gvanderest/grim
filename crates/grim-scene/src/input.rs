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
use grim_core::components::{Account, Client, Name as GrimName};
use grim_networking::{Connection, ConnectionInput, ConnectionOutput};

use crate::command;
use crate::params::{PlayerChars, RoomResolver, SessionRes};
use crate::scene_stack::{top_is_ingame, InGameScene, SceneStack};
use crate::session::JustEnteredWorld;
use crate::sockets::ClientSnapshot;

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_ingame_input(
    mut inputs: MessageReader<ConnectionInput>,
    mut clients: Query<(Entity, &mut Client)>,
    stacks: Query<&SceneStack>,
    ingame: Query<&InGameScene>,
    characters: Query<(Entity, &Character, &Actor, &GrimName)>,
    player_chars: PlayerChars,
    linkdead: Query<&Linkdead>,
    rooms: RoomResolver,
    res: SessionRes,
    connections: Query<&Connection>,
    accounts: Query<&Account>,
    mut just_entered: ResMut<JustEnteredWorld>,
    mut outputs: MessageWriter<ConnectionOutput>,
) {
    // Snapshot every session once per tick for the `sockets` list. A second
    // `Client` query inside `handle_ingame` would conflict with the `&mut`
    // borrow below, so the data crosses as plain values.
    let snapshot: Vec<ClientSnapshot> = clients
        .iter()
        .map(|(entity, c)| ClientSnapshot {
            client: entity,
            connection: c.connection,
            state: c.state.clone(),
            account: c.account,
            character: c.character,
        })
        .collect();
    for ev in inputs.read() {
        let Some(snap) = snapshot.iter().find(|s| s.connection == ev.connection) else {
            continue;
        };
        let Ok((_, mut client)) = clients.get_mut(snap.client) else {
            continue;
        };
        // Only the topmost scene interprets the line. Sessions without an
        // in-game scene on top (pre-game, or mid-transition) are the auth
        // system's job — a missing stack reads as not-in-game, fail closed.
        let in_world = stacks
            .get(snap.client)
            .is_ok_and(|stack| top_is_ingame(stack, |top| ingame.get(top).is_ok()));
        if !in_world {
            continue;
        }
        // Consume-once: this connection was advanced into the world THIS tick by
        // the pre-game system, which already handled the transition line. Swallow
        // exactly that one line — `remove` returns true only for the first input
        // per connection this tick, so a command that arrived in the SAME update
        // after the transition (e.g. ENTER then `look`) still dispatches in order.
        if just_entered.0.remove(&ev.connection) {
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
            &snapshot,
            &connections,
            &accounts,
            &mut outputs,
        );
    }
}
