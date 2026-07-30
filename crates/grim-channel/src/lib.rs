//! Player-speech channels.
//!
//! Today this hosts the say / yell / ooc handlers relocated intact from the
//! former SocialPlugin. The data-driven `add_channel(Channel { scope, identify,
//! eligibility, .. })` model in ARCHITECTURE.md §7 — one shared ChannelMessage
//! event with scope-resolved audience — is deferred with the typed-event command
//! dispatch (§5.2), since it retires the distinct Say/Yell/Ooc events and moves
//! audience resolution across the grim-scene render boundary.

pub mod channel;

pub use channel::ChannelPlugin;
