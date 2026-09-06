//! The object verbs, one file per command. Each module exposes its handler
//! plus a `pub(crate) fn register(app)` that wires that command's systems and
//! the messages it owns; [`crate::plugin::ObjectPlugin`] calls each `register`
//! in turn. The pickup/drop fact ([`ItemEvent`]) is registered once by the
//! plugin — every verb emits it, none owns it.

pub mod get;
pub mod inventory;
