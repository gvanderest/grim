//! The wire messages every transport speaks in terms of, in both directions:
//! transport → session ([`ConnectionEstablished`], [`ConnectionInput`],
//! [`ConnectionClosed`]) and session → transport ([`ConnectionOutput`],
//! [`DisconnectRequest`]).

use bevy::prelude::*;
use std::net::SocketAddr;

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
