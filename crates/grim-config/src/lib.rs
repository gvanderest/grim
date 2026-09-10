//! Player config registry: named settings with valid values and defaults.
//!
//! Plugins register a [`ConfigDef`] (key, accepted values, fallback default,
//! scope); each character's chosen values live on its `Character.config` map
//! (`grim-actor`, persisted via `StoredCharacter`). [`ConfigRegistry::resolve`]
//! reads a choice fail-safe: a stored value applies only when registered and
//! valid, otherwise the default applies (never written back).

mod registry;

pub use registry::{ConfigDef, ConfigRegistry, Scope};
