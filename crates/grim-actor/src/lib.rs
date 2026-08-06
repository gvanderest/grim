//! `grim-actor`: the "beings" of the world and the command verbs that read them.
//!
//! An actor is any entity that can *act* in the world and be placed in a room —
//! player characters today, mobs/NPCs later. This crate owns the being
//! components ([`Character`], [`Player`], [`InRoom`], [`Linkdead`],
//! [`OutputHistory`], [`Role`]) and the being-reading command handlers
//! (`look`, `move`, `goto`, `quit`, `title`, `shutdown`).
//!
//! It depends on `grim-world` (rooms/areas/exits + address lookups) and never
//! the reverse: the world's static topology knows nothing about who stands in
//! it, so `grim-world` is being-free and the actor layer sits above it.

pub mod character;
pub mod commands;
pub mod placement;
pub mod player;
pub mod plugin;

pub use character::{Character, Role};
pub use placement::InRoom;
pub use player::{Linkdead, OutputHistory, Player};
pub use plugin::ActorPlugin;
