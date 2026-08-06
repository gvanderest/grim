//! Engine → client output formatting and per-recipient broadcast: turns game
//! events (look/say/yell/ooc/move/info + announces) into `ConnectionOutput`,
//! captures output for linkdead replay, and fans server broadcasts out.

use bevy::prelude::*;
use grim_actor::{Character, InRoom, OutputHistory, Player};
use grim_engine_types::components::{Description, Name as GrimName};
use grim_engine_types::events::{
    GlobalEcho, InfoMessage, LookEntity, LookRoom, MoveEvent, OocEvent, SayEvent, ServerBroadcast,
    YellEvent,
};
use grim_networking::ConnectionOutput;
use grim_world::{Exits, Room};

use crate::formatter;
use crate::params::AnnounceReaders;

/// Room-occupant query shape, shared by every broadcast helper below.
type Occupants<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static InRoom,
        Option<&'static Player>,
        &'static GrimName,
    ),
>;

#[allow(clippy::too_many_arguments)]
pub(crate) fn format_output(
    mut look_room_events: MessageReader<LookRoom>,
    mut look_entity_events: MessageReader<LookEntity>,
    mut say_events: MessageReader<SayEvent>,
    mut yell_events: MessageReader<YellEvent>,
    mut ooc_events: MessageReader<OocEvent>,
    mut move_events: MessageReader<MoveEvent>,
    mut info_events: MessageReader<InfoMessage>,
    mut gecho_events: MessageReader<GlobalEcho>,
    mut announces: AnnounceReaders,
    rooms: Query<(Entity, &Room, &GrimName)>,
    room_occupants: Occupants,
    room_exits: Query<&Exits>,
    names: Query<&GrimName>,
    descriptions: Query<&Description>,
    characters: Query<&Character>,
    mut outputs: MessageWriter<ConnectionOutput>,
) {
    // ── Login / Logout / Linkdead announces ──
    for ev in announces.login.read() {
        broadcast_global(
            &format!("{} has connected.\n", ev.name),
            &room_occupants,
            &mut outputs,
        );
    }
    for ev in announces.logout.read() {
        broadcast_global(
            &format!("{} has disconnected.\n", ev.name),
            &room_occupants,
            &mut outputs,
        );
    }
    for ev in announces.linkdead.read() {
        let formatted = formatter::format_linkdead(&ev.name, ev.reconnecting);
        broadcast_global(&formatted, &room_occupants, &mut outputs);
    }
    for ev in look_room_events.read() {
        emit_look_room(
            ev,
            &rooms,
            &room_occupants,
            &room_exits,
            &characters,
            &mut outputs,
        );
    }
    for ev in look_entity_events.read() {
        emit_look_entity(ev, &room_occupants, &names, &descriptions, &mut outputs);
    }
    for ev in say_events.read() {
        emit_say(ev, &names, &room_occupants, &mut outputs);
    }
    for ev in yell_events.read() {
        emit_yell(ev, &rooms, &names, &room_occupants, &mut outputs);
    }
    for ev in ooc_events.read() {
        emit_ooc(ev, &names, &room_occupants, &mut outputs);
    }
    for ev in move_events.read() {
        emit_move(ev, &names, &room_occupants, &mut outputs);
    }
    for ev in info_events.read() {
        emit_info(ev, &room_occupants, &mut outputs);
    }
    for ev in gecho_events.read() {
        emit_gecho(ev, &names, &characters, &room_occupants, &mut outputs);
    }
}

/// The connection entity for a recipient, from its `Player`, else the entity.
fn find_conn(target: Entity, room_occupants: &Occupants) -> Entity {
    room_occupants
        .get(target)
        .ok()
        .and_then(|(_, _, p, _)| p.as_ref().and_then(|p| p.connection))
        .unwrap_or(target)
}

fn emit_look_room(
    ev: &LookRoom,
    rooms: &Query<(Entity, &Room, &GrimName)>,
    room_occupants: &Occupants,
    room_exits: &Query<&Exits>,
    characters: &Query<&Character>,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let Ok((_, room, name)) = rooms.get(ev.room) else {
        return;
    };
    let exits = room_exits
        .get(ev.room)
        .ok()
        .map(|e| {
            let mut dirs: Vec<String> = e.exits.keys().map(|d| d.to_string()).collect();
            dirs.sort();
            dirs
        })
        .unwrap_or_default();
    let mut occupant_names: Vec<String> = Vec::new();
    for (e, ir, _, occ_name) in room_occupants.iter() {
        if ir.room == ev.room && e != ev.target {
            occupant_names.push(occ_name.0.clone());
        }
    }
    // Admins see the room's ids in the title for building/debugging.
    let is_admin = characters
        .get(ev.target)
        .map(|c| c.is_admin())
        .unwrap_or(false);
    let grim = room.id.to_string();
    let title = formatter::room_title(
        &name.0,
        is_admin.then_some(formatter::RoomDebugIds {
            entity: ev.room.to_bits(),
            grim: &grim,
            slug: &room.friendly_id,
        }),
    );
    let conn = find_conn(ev.target, room_occupants);
    outputs.write(ConnectionOutput {
        echo: None,
        ..ConnectionOutput::new(
            conn,
            formatter::format_room(&title, &room.description, &exits, &occupant_names),
        )
    });
}

fn emit_look_entity(
    ev: &LookEntity,
    room_occupants: &Occupants,
    names: &Query<&GrimName>,
    descriptions: &Query<&Description>,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let Ok(subj_name) = names.get(ev.subject) else {
        return;
    };
    let desc = descriptions
        .get(ev.subject)
        .map(|d| d.0.clone())
        .unwrap_or_default();
    let conn = find_conn(ev.target, room_occupants);
    outputs.write(ConnectionOutput {
        echo: None,
        ..ConnectionOutput::new(conn, formatter::format_entity(&subj_name.0, &desc))
    });
}

fn emit_say(
    ev: &SayEvent,
    names: &Query<&GrimName>,
    room_occupants: &Occupants,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let Ok(actor_name) = names.get(ev.actor) else {
        return;
    };
    let formatted = formatter::format_say(&actor_name.0, &ev.text);
    broadcast_to_room(ev.room, Some(ev.actor), &formatted, room_occupants, outputs);
}

fn emit_yell(
    ev: &YellEvent,
    rooms: &Query<(Entity, &Room, &GrimName)>,
    names: &Query<&GrimName>,
    room_occupants: &Occupants,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let Ok(actor_name) = names.get(ev.actor) else {
        return;
    };
    let formatted = formatter::format_yell(&actor_name.0, &ev.text);
    let area_rooms: Vec<Entity> = rooms
        .iter()
        .filter(|(_, r, _)| r.area == ev.area)
        .map(|(e, _, _)| e)
        .collect();
    for (entity, ir, player, _) in room_occupants.iter() {
        if !area_rooms.contains(&ir.room) {
            continue;
        }
        if entity == ev.actor {
            continue;
        }
        if let Some(p) = player {
            if let Some(conn) = p.connection {
                outputs.write(ConnectionOutput {
                    prepend_newline: true,
                    ..ConnectionOutput::new(conn, formatted.clone())
                });
            }
        }
    }
}

fn emit_ooc(
    ev: &OocEvent,
    names: &Query<&GrimName>,
    room_occupants: &Occupants,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let Ok(actor_name) = names.get(ev.actor) else {
        return;
    };
    let formatted = formatter::format_ooc(&actor_name.0, &ev.text);
    for (entity, _, player, _) in room_occupants.iter() {
        if entity == ev.actor {
            continue;
        }
        if let Some(p) = player {
            if let Some(conn) = p.connection {
                outputs.write(ConnectionOutput {
                    prepend_newline: true,
                    ..ConnectionOutput::new(conn, formatted.clone())
                });
            }
        }
    }
}

/// Admin `gecho`: broadcast to every connected player in the world, including
/// the sender. Another admin sees it attributed (`Name> text`); the sender and
/// non-admins see the raw text. Rendering is per-recipient, so this cannot be
/// formatted once and broadcast.
fn emit_gecho(
    ev: &GlobalEcho,
    names: &Query<&GrimName>,
    characters: &Query<&Character>,
    room_occupants: &Occupants,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let sender_name = names.get(ev.actor).ok().map(|n| n.0.clone());
    let raw = formatter::format_gecho(None, &ev.text);
    let attributed = sender_name
        .as_deref()
        .map(|n| formatter::format_gecho(Some(n), &ev.text));
    for (entity, _, player, _) in room_occupants.iter() {
        let Some(p) = player else {
            continue;
        };
        let Some(conn) = p.connection else {
            continue;
        };
        let is_other_admin = entity != ev.actor
            && characters
                .get(entity)
                .map(Character::is_admin)
                .unwrap_or(false);
        let text = match (is_other_admin, &attributed) {
            (true, Some(a)) => a.clone(),
            _ => raw.clone(),
        };
        outputs.write(ConnectionOutput {
            prepend_newline: true,
            ..ConnectionOutput::new(conn, text)
        });
    }
}

fn emit_move(
    ev: &MoveEvent,
    names: &Query<&GrimName>,
    room_occupants: &Occupants,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let Ok(actor_name) = names.get(ev.actor) else {
        return;
    };
    let dir_str = ev.direction.to_string();
    let leave_msg = formatter::format_move(&actor_name.0, &dir_str, true);
    broadcast_to_room(ev.from, Some(ev.actor), &leave_msg, room_occupants, outputs);
    let arrive_msg = formatter::format_move(&actor_name.0, &dir_str, false);
    broadcast_to_room(ev.to, Some(ev.actor), &arrive_msg, room_occupants, outputs);
}

fn emit_info(
    ev: &InfoMessage,
    room_occupants: &Occupants,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let conn = find_conn(ev.target, room_occupants);
    outputs.write(ConnectionOutput {
        prepend_newline: true,
        ..ConnectionOutput::new(conn, ev.text.clone())
    });
}

/// Find the Connection entity for a character, using their Player component.
/// Falls back to the input entity if no Player component found.
#[allow(dead_code)]
fn find_connection(entity: Entity, players: &Query<&Player>) -> Entity {
    players
        .get(entity)
        .ok()
        .and_then(|p| p.connection)
        .unwrap_or(entity)
}

/// Capture every `ConnectionOutput` into the connection's `OutputHistory` for
/// linkdead replay on reconnect.
pub(crate) fn capture_output(
    mut output: MessageReader<ConnectionOutput>,
    mut histories: Query<&mut OutputHistory>,
) {
    for ev in output.read() {
        if let Ok(mut history) = histories.get_mut(ev.connection) {
            history.push(&ev.text);
        }
    }
}

/// Send text to every player in the given room, optionally excluding one entity.
fn broadcast_to_room(
    room: Entity,
    exclude: Option<Entity>,
    text: &str,
    occupants: &Occupants,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    for (entity, ir, player, _) in occupants.iter() {
        if ir.room != room {
            continue;
        }
        if Some(entity) == exclude {
            continue;
        }
        if let Some(p) = player {
            if let Some(conn) = p.connection {
                outputs.write(ConnectionOutput {
                    prepend_newline: true,
                    ..ConnectionOutput::new(conn, text.to_string())
                });
            }
        }
    }
}

/// Out-of-band server messages (shutdown warnings) to every connected player.
/// A separate system from `format_output` because that one is already at Bevy's
/// system-parameter ceiling.
pub(crate) fn format_server_broadcast(
    mut broadcasts: MessageReader<ServerBroadcast>,
    occupants: Occupants,
    mut outputs: MessageWriter<ConnectionOutput>,
) {
    for ev in broadcasts.read() {
        broadcast_global(&ev.text, &occupants, &mut outputs);
    }
}

/// Send text to every connected player.
fn broadcast_global(
    text: &str,
    occupants: &Occupants,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    for (_, _, player, _) in occupants.iter() {
        if let Some(p) = player {
            if let Some(conn) = p.connection {
                outputs.write(ConnectionOutput {
                    prepend_newline: true,
                    ..ConnectionOutput::new(conn, text.to_string())
                });
            }
        }
    }
}
