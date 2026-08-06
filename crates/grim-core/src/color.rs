//! Colour re-exports.
//!
//! The colour markup lives in `grim-color`; this module re-exports it so
//! existing call sites (`grim::color::ansi`, `convert_16color`, `escape_codes`,
//! and `grim::color::palette`) keep resolving. The locale-backed `tr`/`tr!` that
//! used to live here moved to the `grim-text` crate.

pub use grim_color::{ansi, convert_16color, escape_codes, palette};
