//! The being-reading command verbs, one file per command. Each module exposes
//! its handler plus a `pub(crate) fn register(app)` that wires that command's
//! systems and the messages it owns; [`crate::plugin::ActorPlugin`] calls each
//! `register` in turn.

pub mod look;
pub mod movement;
pub mod quit;
pub mod shutdown;
pub mod title;
