//! [`GrimNetworkingPlugin`] — registers the wire message types so the rest of
//! the app can read and write them.

use bevy::prelude::*;

use crate::copyover::ConnectionResumed;
use crate::messages::{
    ConnectionClosed, ConnectionEstablished, ConnectionInput, ConnectionOutput, DisconnectRequest,
};

/// Registers the wire message types so the rest of the app can read and write
/// them. A transport (telnet, …) drives the socket on top of these; a headless
/// test harness registers this plugin and injects `ConnectionInput` /
/// drains `ConnectionOutput` directly, with no transport at all.
pub struct GrimNetworkingPlugin;

impl Plugin for GrimNetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<ConnectionResumed>()
            .add_message::<DisconnectRequest>();
    }
}
