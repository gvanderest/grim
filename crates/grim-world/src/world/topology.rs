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

/// Exits on a room entity: direction → destination room entity.
#[derive(Component, Debug, Default)]
pub struct Exits {
    pub exits: HashMap<Cardinal, Entity>,
}

/// Inserted by the seed world system, read by the client during character creation.
#[derive(Resource, Debug)]
pub struct StartingRoom(pub Entity);
