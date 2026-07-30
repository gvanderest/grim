//! example-mud — the vanilla GRIM game a MUD author starts from.
//!
//! The world seed is exposed as a library so the end-to-end test harness
//! (`tests/`) boots the *same* world the binary ships, rather than a
//! divergent copy.

pub mod seed;
