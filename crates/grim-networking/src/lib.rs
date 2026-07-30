//! Transport-agnostic networking primitives.
//!
//! Owns the `Connection` component and the wire events every transport
//! speaks in terms of. A transport crate (e.g. `grim-networking-telnet`)
//! drives the socket and translates to/from these; nothing here knows about
//! sessions, scenes, or the game.
//!
//! The tokio bridge itself still lives in the telnet transport for now — it
//! moves here once a second transport exists and the shared shape is real
//! rather than guessed (see ARCHITECTURE.md §5.1).

use bevy::prelude::*;
use std::net::SocketAddr;

/// A raw network connection. Spawned by a transport.
/// The `id` maps to the transport-side connection identifier.
#[derive(Component, Debug)]
pub struct Connection {
    pub id: usize,
    pub addr: SocketAddr,
    /// Whether the server has sent IAC WILL ECHO (hidden input) for this connection.
    /// Reset to false when the user sends their next input.
    pub echo_hidden: bool,
}

// ─── Transport → session ────────────────────────────────────────────

/// A new connection was accepted and is ready for I/O.
#[derive(Message, Debug)]
pub struct ConnectionEstablished {
    pub connection: Entity,
    pub addr: SocketAddr,
}

/// A line of input arrived on a connection (already framed and filtered).
#[derive(Message, Debug)]
pub struct ConnectionInput {
    pub connection: Entity,
    pub text: String,
}

/// A connection's socket dropped.
#[derive(Message, Debug)]
pub struct ConnectionClosed {
    pub connection: Entity,
}

// ─── Session → transport ────────────────────────────────────────────

/// Text output to a connection. When `echo` is set, the transport toggles
/// terminal echo (IAC WILL/WONT ECHO on telnet) *before* the text, so masking
/// takes effect before the prompt is displayed.
#[derive(Message, Debug)]
pub struct ConnectionOutput {
    pub connection: Entity,
    pub text: String,
    /// If true, a `\n` is prepended before sending (used for unsolicited
    /// game events to avoid appearing on the same line as the user's prompt).
    pub prepend_newline: bool,
    /// If set, toggle terminal echo before sending text.
    /// `true` = enable echo (visible), `false` = disable (hidden, e.g. password).
    pub echo: Option<bool>,
}

impl ConnectionOutput {
    /// Create a new output with the required fields. Optional fields (`echo`,
    /// `prepend_newline`) default to `None` / `false`.
    pub fn new(connection: Entity, text: impl Into<String>) -> Self {
        Self {
            connection,
            text: text.into(),
            echo: None,
            prepend_newline: false,
        }
    }
}

/// Ask the transport to cleanly close a connection.
#[derive(Message, Debug)]
pub struct DisconnectRequest {
    pub connection: Entity,
}

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
            .add_message::<DisconnectRequest>();
    }
}
