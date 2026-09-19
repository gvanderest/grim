//! Input and connection limits for the telnet transport.
//!
//! All time-valued knobs are whole seconds (`u64`). The byte/count knobs
//! bound what one connection may send; the connection knobs bound the
//! accept rate. Enforced in the tokio read task (`bridge`) and the accept
//! loop (`server`); configured on [`TelnetPlugin`](crate::TelnetPlugin) via
//! [`with_limits`](crate::TelnetPlugin::with_limits) or the
//! `telnet_limits` field of `GrimDefaultPlugins`.

use bevy::prelude::*;

/// Cap policy for one telnet listener. Clone-cheap (ten words); the network
/// thread carries a copy and each connection task takes its own.
#[derive(Resource, Clone, Debug)]
pub struct TelnetLimits {
    /// Longest delivered input line in bytes. Longer lines are truncated to
    /// this prefix and the remainder is discarded to the newline, so a long
    /// line can never split into two commands.
    pub max_line_len: usize,
    /// Largest single line (delivered prefix + discarded remainder) in bytes
    /// tolerated without a newline. Past this the sender is slow-dripping
    /// or never terminates: the connection is dropped.
    pub max_buffer: usize,
    /// Most delivered lines allowed per sliding `rate_window_secs`. Past
    /// this the connection is dropped.
    pub max_lines: usize,
    /// Sliding window for `max_lines`, in seconds.
    pub rate_window_secs: u64,
    /// Most new connections accepted inside `total_window_secs` before the
    /// listener sheds (accept-and-immediately-close, no handshake) for
    /// `shed_secs`.
    pub max_connects_total: u32,
    /// Sliding window for `max_connects_total`, in seconds.
    pub total_window_secs: u64,
    /// How long a tripped listener sheds new connections, in seconds.
    pub shed_secs: u64,
}

impl Default for TelnetLimits {
    fn default() -> Self {
        Self {
            max_line_len: 1024,
            max_buffer: 65536,
            max_lines: 30,
            rate_window_secs: 10,
            max_connects_total: 100,
            total_window_secs: 10,
            shed_secs: 3600,
        }
    }
}

impl TelnetLimits {
    /// Effective buffer cap: the configured `max_buffer`, but never below
    /// `max_line_len + 1` — a buffer smaller than one full line plus its
    /// terminator would disconnect every overlong line instead of
    /// truncating it. Enforced here so no invalid state is expressible.
    pub fn buffer_cap(&self) -> usize {
        self.max_buffer.max(self.max_line_len.saturating_add(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_satisfy_the_buffer_invariant() {
        let limits = TelnetLimits::default();
        assert!(limits.max_buffer > limits.max_line_len);
        assert_eq!(limits.buffer_cap(), limits.max_buffer);
    }

    #[test]
    fn buffer_cap_normalizes_a_misconfigured_buffer() {
        let limits = TelnetLimits {
            max_line_len: 1024,
            max_buffer: 100,
            ..TelnetLimits::default()
        };
        assert_eq!(limits.buffer_cap(), 1025);
    }

    #[test]
    fn rate_window_uses_seconds() {
        assert_eq!(TelnetLimits::default().rate_window_secs, 10);
    }
}
