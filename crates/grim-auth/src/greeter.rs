//! The connection-established greeter: the entry point to the pre-game flow.
//! Spawns the session's [`Client`], prints the login banner, and issues the
//! first prompt.

use bevy::prelude::*;
use grim_core::components::Client;
use grim_networking::{ConnectionEstablished, ConnectionOutput};
use grim_text::tr;

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
