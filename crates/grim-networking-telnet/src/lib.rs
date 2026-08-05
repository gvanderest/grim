//! Telnet transport for GRIM.
//!
//! `TelnetPlugin` runs the TCP accept loop on a dedicated tokio runtime thread,
//! bridges it to Bevy's synchronous schedule over channels, negotiates the
//! minimal telnet IAC option set (echo suppression for passwords), renders
//! colour codes to ANSI on the way out, and performs copyover (hot restart) fd
//! handoff to a successor process over a unix socket via `SCM_RIGHTS`.
//!
//! Modules, by concern:
//! - [`server`] — the TCP accept/serve loop and the tokio runtime thread.
//! - [`bridge`] — the tokio↔Bevy channel bridge, connection registry, and the
//!   two Bevy systems draining events and routing outbound messages.
//! - [`iac`] — telnet IAC negotiation and inbound sequence stripping.
//! - [`render`] — the ANSI render path for outbound text.
//! - [`copyover`] — `SIGUSR2` hot restart: listener + socket handoff.

mod bridge;
mod copyover;
mod iac;
mod plugin;
mod render;
mod server;

pub use plugin::TelnetPlugin;
