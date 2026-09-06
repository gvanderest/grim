//! The object verbs, one file per command. Each module exposes its handler
//! plus a `pub(crate) fn register(app)` that wires that command's systems and
pub mod get;
pub mod give;
pub mod inventory;
pub mod steal;
