//! The `TelnetPlugin`: registers the transport wire messages and resources, and
//! schedules the startup + update systems that own the tokio bridge and the
//! copyover lifecycle.

use bevy::prelude::*;
use grim_networking::{
    ConnectionClosed, ConnectionEstablished, ConnectionInput, ConnectionOutput, ConnectionResumed,
    DisconnectRequest,
};

use crate::bridge::{drain_network_events, send_network_commands, TelnetPort};
use crate::copyover::{
    finish_copyover, install_copyover_signal, poll_copyover_signal, CopyoverDone, CopyoverSignal,
};
use crate::server::start_telnet_server;

pub struct TelnetPlugin {
    pub port: u16,
}

impl TelnetPlugin {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
}

impl Plugin for TelnetPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<ConnectionResumed>()
            .add_message::<DisconnectRequest>()
            .insert_resource(TelnetPort(self.port))
            .init_resource::<CopyoverSignal>()
            .init_resource::<CopyoverDone>()
            .add_systems(Startup, (install_copyover_signal, start_telnet_server))
            .add_systems(
                Update,
                (
                    drain_network_events,
                    send_network_commands,
                    poll_copyover_signal,
                    finish_copyover,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telnet_plugin_new() {
        let plugin = TelnetPlugin::new(8080);
        assert_eq!(plugin.port, 8080);

        let plugin2 = TelnetPlugin { port: 9090 };
        assert_eq!(plugin2.port, 9090);
    }
}
