//! Command resolution.
//!
//! A [`CommandRegistry`] maps the first word of a line to the command that
//! registered that name, resolving abbreviations by prefix. It is generic over
//! the produced command type `C`, so it carries no game vocabulary of its own —
//! a caller registers `fn(&str) -> Option<C>` factories and gets `C` back.
//!
//! # Resolution order
//!
//! 1. **Exact match** — a full command name always wins, even when a
//!    higher-priority command shares it as a prefix.
//! 2. **Prefix by priority** — among commands whose name starts with the typed
//!    word, the highest-priority one wins.
//!
//! # Priority
//!
//! Priority is an explicit ordering, not a number. Registering a command puts
//! it at the **front** (highest), so by default the most recently registered
//! command wins a contested prefix — a MUD author installs plugins and the last
//! one to claim `n` wins. To override, call [`CommandRegistry::prioritize`] or
//! [`CommandRegistry::deprioritize`], which move a command to the front or back
//! of the ordering. Nothing else shifts.
//!
//! Because a plugin claiming a prefix can silently change what an abbreviation
//! does, [`CommandRegistry::contested_prefixes`] reports every prefix more than
//! one command answers to, so the ambiguity can be logged at startup rather than
//! discovered by a confused player.
//!
//! Concerns split into modules: [`registry`] (the registry, registration,
//! priority reordering, and exact-then-prefix resolution) and its `contest`
//! child (the [`Contest`] report and [`CommandRegistry::contested_prefixes`]).

mod registry;

pub use registry::{CommandRegistry, Contest};
