//! Player-speech command verbs, one file per command. Each module exposes its
//! handler plus a `pub(crate) fn register(app)` that wires that command's
//! systems and the messages it owns; [`crate::plugin::ChannelPlugin`] calls
//! each `register` in turn.

pub mod gecho;
pub mod ooc;
pub mod reply;
pub mod say;
pub mod tell;
pub mod yell;
