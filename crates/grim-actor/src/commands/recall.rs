//! The player `recall` to the Town Square: a teleport mirroring the admin
//! `goto` in [`super::movement`], but available to every player and fixed on
//! the `haven:square` address. Placement routes through movement's shared
//! [`place_actor`](super::movement::place_actor) seam.

use bevy::log::warn;
use bevy::prelude::*;
use grim_core::events::{Command, EngineCommand, InfoMessage, LookRoom};
use grim_text::tr;
use grim_world::{resolve_room_address, Area, Room, RoomLookup};

use super::movement::{persisted_location, place_actor, PendingFacts, RoomFact};
use crate::character::Character;
use crate::placement::InRoom;
use crate::transition::{AttemptEnter, AttemptLeave};

/// The room `recall` returns to, as an `<area>:<room>` slug address (ADR-0001).
/// Resolved per use via [`resolve_room_address`], so a reseed that changes
/// room entities needs no update here.
const RECALL_ADDRESS: &str = "haven:square";

/// `recall`: return to the Town Square, unless already there. A player escape
/// hatch, so — like the admin `goto` teleport it mirrors — the attempt
/// triggers fire deferred as greetings (denial never consulted) and no
/// `MoveEvent` is emitted. Placement routes through the shared [`place_actor`]
/// seam, the committed `Leave`/`Enter` facts queue into `PendingFacts`, and
/// the destination room is shown. A fixed address that fails to resolve is an
/// internal error: log it and fail closed with [`InfoMessage`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_recall(
    mut engine: MessageReader<EngineCommand>,
    mut commands: Commands,
    mut inroom: Query<&mut InRoom>,
    rooms: Query<(Entity, &Room)>,
    areas: Query<(Entity, &Area)>,
    mut characters: Query<&mut Character>,
    mut look_room: MessageWriter<LookRoom>,
    mut info: MessageWriter<InfoMessage>,
    mut pending: ResMut<PendingFacts>,
) {
    for cmd in engine.read() {
        let Command::Recall = cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let RoomLookup::Found(to) = resolve_room_address(RECALL_ADDRESS, &rooms, &areas) else {
            warn!("recall: address '{RECALL_ADDRESS}' did not resolve to one room");
            info.write(InfoMessage {
                target: actor,
                text: tr!("recall.failed"),
            });
            continue;
        };
        // No placement, no recall — like `handle_move`, skip rather than
        // erroring (a `place_actor` here would rewrite `last_room` and show
        // a room the actor is not in).
        let Some(from) = inroom.get(actor).map(|ir| ir.room).ok() else {
            continue;
        };
        if from == to {
            info.write(InfoMessage {
                target: actor,
                text: tr!("recall.already"),
            });
            continue;
        }
        let loc = rooms.get(to).ok().and_then(|(_, r)| {
            areas
                .get(r.area)
                .ok()
                .map(|(_, a)| persisted_location(r, a))
        });
        // Greetings, not veto: recall always works, so the attempts fire
        // (deferred) but denial is never consulted.
        commands.trigger(AttemptLeave {
            actor,
            room: from,
            denied: false,
        });
        commands.trigger(AttemptEnter {
            actor,
            room: to,
            denied: false,
        });
        place_actor(actor, to, loc, &mut inroom, &mut characters);
        pending.incoming.push(RoomFact { actor, from, to });
        look_room.write(LookRoom {
            target: actor,
            room: to,
        });
    }
}

/// Wire the `recall` handler and the input/delivery messages it owns. The
/// room-transition moments are trigger *events* (`AttemptLeave`/`AttemptEnter`/
/// `Leave`/`Enter`), which need no registration. The world-happening event it
/// emits (`LookRoom`) is registered by `grim_world::WorldPlugin`, and the
/// `PendingFacts` resource by `super::movement::register` (both composed
/// alongside this in `ActorPlugin`).
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_systems(Update, handle_recall);
}
