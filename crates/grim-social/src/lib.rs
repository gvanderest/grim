//! Data-driven socials (`grin`, `smile`, `wave`, …): emote commands whose
//! message variants live in `data/socials/<name>.json` and fall back to
//! built-in defaults.
//!
//! A social has three cases — solo (no target), self (target is the actor),
//! other (target is someone else) — each with per-audience wordings. The
//! pattern lives in the JSON file so builders reskin a verb by copying one.
//! Names are registered into the shared [`grim_command::CommandRegistry`] at
//! startup (after the static commands, so statics keep their prefixes); the
//! handler renders per-recipient [`grim_core::events::InfoMessage`]s, the same
//! primitive `tell` uses.

mod handler;
mod plugin;
mod social;

pub use handler::SocialPerformed;
pub use plugin::SocialPlugin;
pub use social::{SocialDef, SocialDir, SocialRegistry, BUILTIN_SOCIALS};
