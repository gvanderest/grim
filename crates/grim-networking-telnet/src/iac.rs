//! Telnet IAC (Interpret As Command) negotiation: the minimal option handshake
//! sent on a fresh accept, the echo on/off commands used for password entry, and
//! the inbound IAC-sequence stripper.

/// Initial negotiation sent to a freshly-accepted socket: `IAC WILL ECHO`,
/// `IAC WILL SUPPRESS_GO_AHEAD`. Re-adopted copyover sockets skip this — they
/// already negotiated with the predecessor.
pub(crate) const HANDSHAKE: [u8; 6] = [255, 253, 1, 255, 253, 3];

/// `IAC WILL ECHO` — the server takes over echoing, so the client shows nothing:
/// hidden input for password entry.
pub(crate) const WILL_ECHO: [u8; 3] = [255, 251, 1];

/// `IAC WONT ECHO` — the server relinquishes echo, so the client shows typed
/// input again: visible input (the normal state).
pub(crate) const WONT_ECHO: [u8; 3] = [255, 252, 1];

/// Strip inbound telnet IAC sequences (`0xFF cmd1 cmd2`) from a raw line,
/// returning only the payload bytes.
pub(crate) fn strip_iac(buf: &[u8]) -> Vec<u8> {
    let mut clean = Vec::with_capacity(buf.len());
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == 0xFF && i + 2 < buf.len() {
            i += 3;
            continue;
        }
        clean.push(buf[i]);
        i += 1;
    }
    clean
}
