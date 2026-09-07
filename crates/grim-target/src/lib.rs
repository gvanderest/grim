//! `grim-target`: parse a textual `<target>` into a [`TargetSpec`] and resolve
//! it against candidate entities.
//!
//! Three pieces, used in order by every verb that names something:
//!
//! 1. [`parse_target`] (`parse`) — split the raw string into lowercased terms
//!    plus a [`Selector`], gated by [`ParseOptions`] (`ITEM` for things,
//!    `BEING` for beings).
//! 2. [`rank_match`] (`rank`) — score one name (+ keywords) against the terms:
//!    exact name, exact keyword, then prefix; multi-term groups AND at the
//!    prefix tier.
//! 3. [`query`] (`query`) — rank a candidate iterator best-first and apply the
//!    selector (`2.potion` → the second, `3*coin` → three, `all` → all).
//!
//! A plain library (Bevy only, for `Entity` ordering) — no plugin, no game
//! types, so `grim-actor`, `grim-object`, and `grim-channel` all share it.

pub mod parse;
pub mod query;
pub mod rank;

pub use parse::{parse_target, split_target_terms, ParseOptions, Selector, TargetSpec};
pub use query::query;
pub use rank::{rank_match, Rank};
