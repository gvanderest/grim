//! Channel message output formatting and per-recipient broadcast.

use bevy::prelude::*;
use grim_actor::{Character, Player};
use grim_channel::{ChannelMessage, ChannelRegistry};
use grim_core::channel::{ListenEligibility, Scope};
use grim_core::components::Name as GrimName;
use grim_networking::ConnectionOutput;
use grim_world::Room;

use crate::formatter;
use crate::output::Occupants;

/// Emit a channel message to the appropriate audience based on channel configuration.
#[allow(clippy::too_many_arguments)]
pub fn emit_channel(
    ev: &ChannelMessage,
    _channel_registry: &ChannelRegistry,
    names: &Query<&GrimName>,
    room_occupants: &Occupants,
    rooms: &Query<(Entity, &Room, &GrimName)>,
    characters: &Query<&Character>,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let channel = &ev.channel;

    // Look up the actor's name
    let Ok(actor_name) = names.get(ev.actor) else {
        return;
    };

    // Get the catalog key from the channel config
    let catalog_key = &channel.key;

    // Format the message using the catalog (third_party for recipients)
    let formatted = formatter::format_channel_message(catalog_key, &actor_name.0, &ev.text);

    // Resolve the audience based on channel scope
    let mut audience = Vec::new();

    match channel.scope {
        Scope::Room => {
            // Message goes to everyone in the actor's room
            if let Ok((_, ir, _, _, _, _)) = room_occupants.get(ev.actor) {
                let actor_room = ir.room;
                // Find all entities in the same room
                for (entity, ir, _, _, _, _) in room_occupants.iter() {
                    if ir.room == actor_room {
                        audience.push(entity);
                    }
                }
            }
        }
        Scope::Area => {
            // Message goes to everyone in the actor's area
            if let Ok((_, ir, _, _, _, _)) = room_occupants.get(ev.actor) {
                if let Ok((_, room, _)) = rooms.get(ir.room) {
                    let actor_area = room.area;
                    // Find all entities in rooms in this area
                    for (entity, ir, _, _, _, _) in room_occupants.iter() {
                        if let Ok((_, room, _)) = rooms.get(ir.room) {
                            if room.area == actor_area {
                                audience.push(entity);
                            }
                        }
                    }
                }
            }
        }
        Scope::Global => {
            // Message goes to all connected players
            for (entity, _, _, _, _, _) in room_occupants.iter() {
                audience.push(entity);
            }
        }
    }

    // Send to each audience member (excluding the actor), checking listen eligibility
    for entity in audience {
        if entity == ev.actor {
            continue;
        }

        // Check if entity is in the world (has Player or Linkdead)
        if let Ok((_, _ir, player, _, _, _)) = room_occupants.get(entity) {
            // Check listen eligibility
            if !check_listen_eligibility(&channel.listen, entity, &player, _ir, characters) {
                continue;
            }

            if let Some(p) = player {
                outputs.write(ConnectionOutput {
                    prepend_newline: true,
                    ..ConnectionOutput::new(p.connection, formatted.clone())
                });
            }
        }
    }
}

/// Check if an entity can listen to a channel based on listen eligibility.
fn check_listen_eligibility(
    listen: &ListenEligibility,
    entity: Entity,
    player: &Option<&Player>,
    _ir: &grim_actor::InRoom,
    characters: &Query<&Character>,
) -> bool {
    match listen {
        ListenEligibility::All => true,
        ListenEligibility::AdminOnly => {
            // Admin check - need Character with admin role
            characters
                .get(entity)
                .map(Character::is_admin)
                .unwrap_or(false)
        }
        ListenEligibility::Authenticated => {
            // Must have Player (authenticated) or Linkdead (still in world)
            player.is_some()
        }
        ListenEligibility::InRoom => {
            // Checked at scope resolution time
            true
        }
        ListenEligibility::InArea => {
            // Checked at scope resolution time
            true
        }
    }
}
