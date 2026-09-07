//! The connection-established greeter: the entry point to the pre-game flow.
//! Spawns the session's [`Client`], prints the login banner, and issues the
//! first prompt.

use bevy::prelude::*;
use grim_core::components::Client;
use grim_networking::{ConnectionEstablished, ConnectionOutput, DisconnectRequest};
use grim_persistence::BanList;
use grim_text::tr;

pub(crate) fn handle_connection_established(
    mut established: MessageReader<ConnectionEstablished>,
    mut commands: Commands,
    mut outputs: MessageWriter<ConnectionOutput>,
    bans: Res<BanList>,
    mut disconnect: MessageWriter<DisconnectRequest>,
) {
    for ev in established.read() {
        // Banned IPs never get a session: refuse with the ban message and
        // sever the socket before any `Client` exists to drive it.
        if bans.is_ip_banned(&ev.addr.ip()) {
            outputs.write(ConnectionOutput::new(ev.connection, tr!("ban.banned.ip")));
            disconnect.write(DisconnectRequest {
                connection: ev.connection,
            });
            continue;
        }
        commands.spawn(Client::new(ev.connection));
        let banner = grim_color::ansi(include_str!("../../../assets/login-banner.txt"));
        let text = format!("{}\n\n{}", banner, tr!("login.prompt"));
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(ev.connection, text)
        });
    }
}
