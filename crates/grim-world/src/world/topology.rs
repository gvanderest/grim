//! World topology: the static spatial structure of the game — areas, rooms,
//! their exits, and the resource naming the room new characters spawn into.
//!
//! These types moved out of the `grim-core` god-node (Placement Phase
//! 2a) so the world's own data lives in the world crate. They depend only on
//! primitives ([`GrimId`], [`Cardinal`]) that still live downward in
//! `grim-core`.

use std::collections::HashMap;

use bevy::prelude::*;
use grim_core::cardinal::Cardinal;
use grim_core::id::GrimId;

/// An area — a collection of rooms. Friendly ID is filesystem-unique.
#[derive(Component, Debug)]
pub struct Area {
    pub id: GrimId,
    pub friendly_id: String,
    pub name: String,
}

/// A room — belongs to an area, has exits.
#[derive(Component, Debug)]
pub struct Room {
    pub id: GrimId,
    pub friendly_id: String,
    pub name: String,
    pub description: String,
    pub area: Entity,
}

/// A door hung on one side of an exit: its display name, look keywords,
/// whether passage is currently allowed, and whether the exit is hidden from
/// that side. Both linked rooms carry their own copy; `open`/`close` flip the
/// pair (see `grim-actor` doors handler). `hidden` is per-side metadata —
/// never flipped — so a one-way secret has `hidden: true` on one side and
/// `hidden: false` on the other.
#[derive(Debug, Clone)]
pub struct Door {
    pub name: String,
    pub keywords: Vec<String>,
    pub open: bool,
    /// Hidden from this side: omitted from `Exits:`/`Doors:` for players,
    /// shown under `Secret:` for admins; a closed hidden door refuses walks
    /// as "You can't go that way." (identical to no exit).
    pub hidden: bool,
}

/// Doors on a room entity: direction → the door guarding that exit. A
/// direction absent here has no door (free passage); present-but-closed
/// blocks `move`. A present-but-hidden exit is invisible to players (listed
/// under `Secret:` for admins only).
#[derive(Component, Debug, Default)]
pub struct Doors {
    pub doors: HashMap<Cardinal, Door>,
}

/// Exits on a room entity: direction → destination room entity.
#[derive(Component, Debug, Default)]
pub struct Exits {
    pub exits: HashMap<Cardinal, Entity>,
}

/// Inserted by the seed world system, read by the client during character creation.
#[derive(Resource, Debug)]
pub struct StartingRoom(pub Entity);
