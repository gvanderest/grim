//! Movement: walking an exit (`move`) and the admin `goto` teleport, plus the
//! shared [`place_actor`] seam every "put actor in room X" path routes through.

use bevy::prelude::*;
use grim_engine_types::components::{Character, InRoom, RoomLocation};
use grim_engine_types::events::{Command, EngineCommand, InfoMessage, LookRoom, MoveEvent};

use super::area::{resolve_room_address, room_location, RoomLookup};
use super::topology::{Area, Exits, Room};

/// The single seam every "put actor in room X" path routes through: set the
/// actor's `InRoom` and refresh their persisted location. Per ADR-0001 the
/// location update is a property of the *destination*, not of how the actor
/// arrived, so walk and `goto` (and, later, summon/recall/login) share it. `loc`
/// is the destination's persisted [`RoomLocation`], precomputed by the caller.
///
/// Persists only `last_room` today. ADR-0001's `last_canonical_room` is not a
/// field yet; while every room is Canonical (no instancing) the two would be
/// equal, so it is deferred to the instancing work rather than added dead here.
fn place_actor(
    actor: Entity,
    to: Entity,
    loc: Option<RoomLocation>,
    inroom: &mut Query<&mut InRoom>,
    characters: &mut Query<&mut Character>,
) {
    if let Ok(mut ir) = inroom.get_mut(actor) {
        ir.room = to;
    }
    if let Some(loc) = loc {
        if let Ok(mut character) = characters.get_mut(actor) {
            character.last_room = Some(loc);
        }
    }
}

/// `move <direction>`: traverse an exit, emitting a movement event and an
/// automatic look at the destination. Also refreshes the character's persisted
/// `last_room` so a restart/copyover resumes them where they walked to.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_move(
    mut engine: MessageReader<EngineCommand>,
    mut inroom: Query<&mut InRoom>,
    exits: Query<&Exits>,
    rooms: Query<&Room>,
    areas: Query<&Area>,
    mut characters: Query<&mut Character>,
    mut move_ev: MessageWriter<MoveEvent>,
    mut look_room: MessageWriter<LookRoom>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Move { direction } = cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let from = match inroom.get(actor) {
            Ok(ir) => ir.room,
            Err(_) => continue,
        };
        match exits.get(from) {
            Ok(room_exits) => match room_exits.exits.get(&direction).copied() {
                Some(to) => {
                    // Keep the persisted location current on every step so an
                    // unexpected restart or copyover resumes the character in the
                    // room they actually walked to, not a stale one.
                    let loc = room_location(to, &rooms, &areas);
                    place_actor(actor, to, loc, &mut inroom, &mut characters);
                    move_ev.write(MoveEvent {
                        actor,
                        from,
                        to,
                        direction,
                    });
                    look_room.write(LookRoom {
                        target: actor,
                        room: to,
                    });
                }
                None => {
                    info.write(InfoMessage {
                        target: actor,
                        text: "You can't go that way.\n".into(),
                    });
                }
            },
            Err(_) => {
                info.write(InfoMessage {
                    target: actor,
                    text: "You can't go that way.\n".into(),
                });
            }
        }
    }
}

/// `goto <address>`: admin-only teleport. Resolves the address through
/// [`resolve_room_address`] and places the actor via the shared [`place_actor`]
/// seam, then shows the destination room.
///
/// Admin-gated here as defense in depth: the dispatcher already masks `goto` as
/// an unknown command for non-admins, so a well-behaved session never sends this
/// for one. A `goto` from a non-client source with no admin character is refused
/// silently (emitting anything would leak that the command exists).
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_goto(
    mut engine: MessageReader<EngineCommand>,
    mut inroom: Query<&mut InRoom>,
    rooms: Query<(Entity, &Room)>,
    areas: Query<(Entity, &Area)>,
    mut characters: Query<&mut Character>,
    mut look_room: MessageWriter<LookRoom>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Goto { target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let is_admin = characters.get(actor).map(|c| c.is_admin()).unwrap_or(false);
        if !is_admin {
            continue;
        }
        match resolve_room_address(target, &rooms, &areas) {
            RoomLookup::Found(to) => {
                let loc = rooms.get(to).ok().and_then(|(_, r)| {
                    areas.get(r.area).ok().map(|(_, a)| RoomLocation {
                        area: a.friendly_id.clone(),
                        room: r.friendly_id.clone(),
                    })
                });
                place_actor(actor, to, loc, &mut inroom, &mut characters);
                look_room.write(LookRoom {
                    target: actor,
                    room: to,
                });
            }
            RoomLookup::NotFound => {
                // `target` is raw admin input; escape it so it can't inject
                // colour markup into the reply (only the admin's own session,
                // but keep the invariant that interpolated input is escaped).
                info.write(InfoMessage {
                    target: actor,
                    text: format!("No room matches '{}'.\n", grim_color::escape_codes(target)),
                });
            }
            RoomLookup::Ambiguous(candidates) => {
                // List every candidate with its distinguishing ids so the admin
                // can re-issue `goto` against a unique one (an entity or grim id).
                let mut text = String::from("Select an option...\n");
                for e in candidates {
                    if let Ok((_, r)) = rooms.get(e) {
                        text.push_str(&room_ident_line(e, r));
                        text.push('\n');
                    }
                }
                info.write(InfoMessage {
                    target: actor,
                    text,
                });
            }
        }
    }
}

/// One disambiguation line for a room: `Name (entity:… grim:… slug:…)`. Matches
/// the admin room-title debug format. (A future instance id would slot in here.)
fn room_ident_line(entity: Entity, room: &Room) -> String {
    format!(
        "{} (entity:{} grim:{} slug:{})",
        room.name,
        entity.to_bits(),
        room.id,
        room.friendly_id
    )
}
