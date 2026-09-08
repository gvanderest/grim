//! `grim-script`: sandboxed Lua triggers on room transitions.
//!
//! Creatures carry [`ScriptTriggers`]: an ordered list of ([`TriggerKind`],
//! Lua source) pairs authored inline in the mob blueprint. When a being enters
//! or leaves a room, every scripted creature in the affected room fires its
//! matching triggers. Each firing runs in a fresh Lua state holding exactly
//! three globals — `rand`, `self` (the observing entity), and `event` — over
//! a pure-data
//! stdlib (`math`, `string`, `table`, `utf8`); there is no `os`/`io`/
//! `require`/`load`, an instruction budget, and a memory cap, so a bad script
//! can neither escape nor hang the tick.
//!
//! It layers on `grim-actor` (the transition events + being types) and
//! `grim-channel` (mob speech goes out as a `say` [`ChannelMessage`]), never
//! the reverse.

pub mod plugin;
pub mod runtime;
pub mod trigger;
mod watch;

pub use plugin::ScriptPlugin;
pub use runtime::{run_trigger, Outcome};
pub use trigger::{compile, CompiledTrigger, ScriptTriggers, TriggerDef, TriggerKind};
