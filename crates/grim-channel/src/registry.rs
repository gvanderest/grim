//! Channel registry: stores channel configurations and resolves audience.

use std::collections::HashMap;

use bevy::prelude::*;

use grim_actor::InRoom;
use grim_core::channel::{Channel, Scope};
use grim_world::Room;

/// Resource that holds all registered channels.
#[derive(Resource, Default)]
pub struct ChannelRegistry {
    channels: HashMap<String, Channel>,
}

impl ChannelRegistry {
    /// Register a new channel.
    pub fn add_channel(&mut self, channel: Channel) {
        self.channels.insert(channel.name.clone(), channel);
    }

    /// Get a channel by name.
    pub fn get(&self, name: &str) -> Option<&Channel> {
        self.channels.get(name)
    }

    /// Iterate over all registered channels.
    pub fn iter(&self) -> impl Iterator<Item = &Channel> {
        self.channels.values()
    }
}

/// Resolve the audience for a channel message based on the channel configuration.
pub fn resolve_audience(
    channel: &Channel,
    actor: Entity,
    inroom: &Query<(Entity, &InRoom)>,
    rooms: &Query<&Room>,
) -> Vec<Entity> {
    let mut audience = Vec::new();

    match channel.scope {
        Scope::Room => {
            // Message goes to everyone in the actor's room
            if let Ok((_actor_entity, ir)) = inroom.get(actor) {
                let actor_room = ir.room;
                // Find all entities in the same room
                for (entity, ir) in inroom.iter() {
                    if ir.room == actor_room {
                        audience.push(entity);
                    }
                }
            }
        }
        Scope::Area => {
            // Message goes to everyone in the actor's area
            if let Ok((_actor_entity, ir)) = inroom.get(actor) {
                if let Ok(room) = rooms.get(ir.room) {
                    let actor_area = room.area;
                    // Find all entities in rooms in this area
                    for (entity, ir) in inroom.iter() {
                        if let Ok(room) = rooms.get(ir.room) {
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
            for (entity, _) in inroom.iter() {
                audience.push(entity);
            }
        }
    }

    audience
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_registry_adds_channel() {
        let mut registry = ChannelRegistry::default();
        let channel = Channel::new("test");
        registry.add_channel(channel.clone());
        assert_eq!(registry.get("test"), Some(&channel));
    }

    #[test]
    fn channel_registry_get_missing() {
        let registry = ChannelRegistry::default();
        assert_eq!(registry.get("nonexistent"), None);
    }

    #[test]
    fn channel_registry_iter() {
        let mut registry = ChannelRegistry::default();
        registry.add_channel(Channel::new("channel1"));
        registry.add_channel(Channel::new("channel2"));
        assert_eq!(registry.iter().count(), 2);
    }
}
