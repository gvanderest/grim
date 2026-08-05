//! Rooms / areas / exits: the world's static structure and the shared lookups
//! that resolve a room *address* (and a room's stable storage location) against
//! it. Movement and admin `goto` build on these; see `docs/adr/0001`.

use bevy::prelude::*;
use grim_engine_types::components::{Area, Room, RoomLocation};

/// Resolve a room entity to its stable, entity-independent storage location
/// (area + room `friendly_id`s). These survive a world reseed, so persisting
/// them lets a character be placed back into the *new* instance of the same room
/// after a restart or copyover — see `grim-scene`'s placement resolver.
pub fn room_location(
    room: Entity,
    rooms: &Query<&Room>,
    areas: &Query<&Area>,
) -> Option<RoomLocation> {
    let r = rooms.get(room).ok()?;
    let area = areas.get(r.area).ok()?;
    Some(RoomLocation {
        area: area.friendly_id.clone(),
        room: r.friendly_id.clone(),
    })
}

/// Outcome of resolving a room [address](resolve_room_address).
#[derive(Debug, PartialEq, Eq)]
pub enum RoomLookup {
    /// Exactly one room matched.
    Found(Entity),
    /// Nothing matched the address.
    NotFound,
    /// A slug matched more than one room (e.g. several instances of an area).
    /// Carries every candidate so the caller can list them for disambiguation.
    Ambiguous(Vec<Entity>),
}

/// Resolve a room *address* to a room entity — the shared lookup behind admin
/// `goto` and (later) other targeting. See `docs/adr/0001`.
///
/// Precedence, most specific first: an **entity id** (`Entity::to_bits` as a
/// decimal, boot-local), then a **grim id** (globally unique), then a **slug**
/// (`friendly_id`). An address is either `<area>:<room>` — each side
/// independently an entity id, grim id, or slug — or a bare room token. A bare
/// slug that matches rooms in several areas is [`Ambiguous`](RoomLookup::Ambiguous)
/// (grim ids never are).
pub fn resolve_room_address(
    input: &str,
    rooms: &Query<(Entity, &Room)>,
    areas: &Query<(Entity, &Area)>,
) -> RoomLookup {
    let input = input.trim();
    if input.is_empty() {
        return RoomLookup::NotFound;
    }

    if let Some((area_tok, room_tok)) = input.split_once(':') {
        let Some(area) = resolve_area(area_tok.trim(), areas) else {
            return RoomLookup::NotFound;
        };
        return resolve_room_in_area(room_tok.trim(), area, rooms);
    }

    // Bare token. Entity id is most specific.
    if let Some(e) = parse_entity(input) {
        if rooms.get(e).is_ok() {
            return RoomLookup::Found(e);
        }
        // A numeric token that isn't a live room falls through — a grim id
        // could in principle be all digits — rather than hard-failing.
    }

    // Grim ID: globally unique, so an exact match wins outright.
    if let Some((e, _)) = rooms.iter().find(|(_, r)| r.id.as_str() == input) {
        return RoomLookup::Found(e);
    }

    // Slug: a room `friendly_id`, which is unique only within its area.
    let hits: Vec<Entity> = rooms
        .iter()
        .filter(|(_, r)| r.friendly_id.eq_ignore_ascii_case(input))
        .map(|(e, _)| e)
        .collect();
    classify(hits)
}

/// Turn a list of slug candidates into a lookup outcome.
fn classify(mut hits: Vec<Entity>) -> RoomLookup {
    match hits.len() {
        0 => RoomLookup::NotFound,
        1 => RoomLookup::Found(hits.remove(0)),
        _ => RoomLookup::Ambiguous(hits),
    }
}

/// Resolve the area side of an `<area>:<room>` address to an area entity.
fn resolve_area(tok: &str, areas: &Query<(Entity, &Area)>) -> Option<Entity> {
    if let Some(e) = parse_entity(tok) {
        if areas.get(e).is_ok() {
            return Some(e);
        }
    }
    // Grim id (globally unique) before slug.
    if let Some((e, _)) = areas.iter().find(|(_, a)| a.id.as_str() == tok) {
        return Some(e);
    }
    areas
        .iter()
        .find(|(_, a)| a.friendly_id.eq_ignore_ascii_case(tok))
        .map(|(e, _)| e)
}

/// Resolve the room side of an `<area>:<room>` address within a known area.
fn resolve_room_in_area(tok: &str, area: Entity, rooms: &Query<(Entity, &Room)>) -> RoomLookup {
    if let Some(e) = parse_entity(tok) {
        if rooms.get(e).map(|(_, r)| r.area == area).unwrap_or(false) {
            return RoomLookup::Found(e);
        }
    }
    // Grim id (globally unique) before slug.
    if let Some((e, _)) = rooms
        .iter()
        .find(|(_, r)| r.area == area && r.id.as_str() == tok)
    {
        return RoomLookup::Found(e);
    }
    // Slug within the area — may match multiple instances of the same room.
    let hits: Vec<Entity> = rooms
        .iter()
        .filter(|(_, r)| r.area == area && r.friendly_id.eq_ignore_ascii_case(tok))
        .map(|(e, _)| e)
        .collect();
    classify(hits)
}

/// Parse an entity-id token (`Entity::to_bits` decimal) into a well-formed
/// entity, or `None` if it is not a valid bit pattern. Whether it is *live* is
/// the caller's check.
fn parse_entity(tok: &str) -> Option<Entity> {
    tok.parse::<u64>().ok().and_then(Entity::try_from_bits)
}
