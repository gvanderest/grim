//! Room-look rendering: the room title + exits + presence block (`emit_look_room`)
//! and single-entity descriptions (`emit_look_entity`). Factored out of
//! `output.rs` (file-length cap) — the same one-concern-per-file shape as
//! `recall_output.rs` and `item_output.rs`.

use std::collections::HashMap;

use bevy::prelude::*;
use grim_actor::{Character, Linkdead};
use grim_config::ConfigRegistry;
use grim_core::components::{Client, Description, Name as GrimName};
use grim_core::events::{LookEntity, LookRoom};
use grim_networking::ConnectionOutput;
use grim_text::tr;
use grim_world::{render_map, Exits, MapConfig, Room};

use crate::formatter;
use crate::output::{find_conn, Occupants};

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_look_room(
    ev: &LookRoom,
    rooms: &Query<(Entity, &Room, &GrimName)>,
    room_occupants: &Occupants,
    room_exits: &Query<(Entity, &Exits)>,
    characters: &Query<&Character>,
    clients: &Query<&Client>,
    linkdead_chars: &Query<&Linkdead>,
    config_registry: &ConfigRegistry,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let Ok((_, room, name)) = rooms.get(ev.room) else {
        return;
    };
    let exits = room_exits
        .get(ev.room)
        .ok()
        .map(|(_, e)| {
            let mut dirs: Vec<String> = e.exits.keys().map(|d| d.to_string()).collect();
            dirs.sort();
            dirs
        })
        .unwrap_or_default();
    let presence = collect_presence(
        ev.room,
        ev.target,
        room_occupants,
        characters,
        clients,
        linkdead_chars,
    );
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
    let body = formatter::format_room(&title, &room.description, &exits, &presence);
    // Minimap: the same renderer as `map` on the small canvas, stapled left
    // when the looker's resolved `minimap` setting is on. Actors without a
    // `Character` and unseeded registries (unit tests) keep the map, matching
    // the pre-toggle behavior.
    let minimap_on = characters
        .get(ev.target)
        .map(|c| {
            config_registry
                .resolve(&c.config, "minimap")
                .unwrap_or("on")
                == "on"
        })
        .unwrap_or(true);
    let text = if minimap_on {
        let mut snapshot = HashMap::new();
        for (room_entity, links) in room_exits.iter() {
            snapshot.insert(room_entity, links.exits.clone());
        }
        let map_rows = render_map(ev.room, &snapshot, &MapConfig::MINIMAP);
        formatter::staple_minimap(&map_rows, &body)
    } else {
        body
    };
    outputs.write(ConnectionOutput {
        echo: None,
        ..ConnectionOutput::new(conn, text)
    });
}

/// Presence lines, one per other being — plus one per ground object — in the
/// room: player characters (standing line, position-driven once positions
/// exist) sort above creatures (their long room line, or a plain fallback),
/// with objects (their long room line, or the short name) last. Each group
/// sorts by name so the listing is deterministic. Objects share the presence
/// block with no blank line between the groups. Factored out of
/// [`emit_look_room`] for the line budget.
#[allow(clippy::too_many_arguments)]
fn collect_presence(
    room: Entity,
    target: Entity,
    room_occupants: &Occupants,
    characters: &Query<&Character>,
    clients: &Query<&Client>,
    linkdead_chars: &Query<&Linkdead>,
) -> Vec<String> {
    let mut players: Vec<(String, String)> = Vec::new();
    let mut creatures: Vec<(String, String)> = Vec::new();
    let mut objects: Vec<(String, String)> = Vec::new();
    for (e, ir, player, occ_name, room_line, is_object) in room_occupants.iter() {
        if ir.room != room || e == target {
            continue;
        }
        if characters.get(e).is_ok() {
            // Linkdead dominates (a linkdead character has no session, so it
            // can never also be AFK); otherwise an AFK session reads marked.
            // Literal keys (not a variable): the tr-coverage check resolves
            // every catalog reference statically.
            if linkdead_chars.get(e).is_ok() {
                players.push((
                    occ_name.0.clone(),
                    tr!(
                        "room.presence.standing_linkdead",
                        name = occ_name.0.as_str()
                    ),
                ));
            } else {
                let afk = player
                    .and_then(|p| clients.iter().find(|c| c.connection == p.connection))
                    .is_some_and(|c| c.afk);
                if afk {
                    players.push((
                        occ_name.0.clone(),
                        tr!("room.presence.standing_afk", name = occ_name.0.as_str()),
                    ));
                } else {
                    players.push((
                        occ_name.0.clone(),
                        tr!("room.presence.standing", name = occ_name.0.as_str()),
                    ));
                }
            }
        } else if is_object.is_some() {
            // Ground only: carried objects have no `InRoom`, so they never
            // reach this loop.
            match room_line.filter(|l| !l.0.is_empty()) {
                Some(line) => objects.push((occ_name.0.clone(), line.0.clone())),
                None => objects.push((occ_name.0.clone(), occ_name.0.clone())),
            }
        } else if let Some(line) = room_line.filter(|l| !l.0.is_empty()) {
            creatures.push((occ_name.0.clone(), line.0.clone()));
        } else {
            creatures.push((
                occ_name.0.clone(),
                tr!("room.presence.here", name = occ_name.0.as_str()),
            ));
        }
    }
    players.sort();
    creatures.sort();
    objects.sort();
    players
        .into_iter()
        .chain(creatures)
        .chain(objects)
        .map(|(_, line)| line)
        .collect()
}

pub(crate) fn emit_look_entity(
    ev: &LookEntity,
    room_occupants: &Occupants,
    names: &Query<&GrimName>,
    descriptions: &Query<&Description>,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let Ok(subj_name) = names.get(ev.subject) else {
        return;
    };
    // Entries hold no newlines themselves; each warrants one between them.
    let desc = descriptions
        .get(ev.subject)
        .map(|d| d.0.join("\n"))
        .unwrap_or_default();
    let conn = find_conn(ev.target, room_occupants);
    outputs.write(ConnectionOutput {
        echo: None,
        ..ConnectionOutput::new(conn, formatter::format_entity(&subj_name.0, &desc))
    });
}
