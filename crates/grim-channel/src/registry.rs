//! Channel registry: stores channel configurations and resolves audience.

use std::collections::HashMap;

use bevy::prelude::*;

use grim_actor::{in_room, InRoom};
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
///
/// Room scope funnels through the shared [`in_room`] helper; the area arm
/// walks [`Room::area`] (ADR-0001). Call sites with richer tuples map to
/// `(Entity, room)` pairs instead of reimplementing the predicate.
pub fn resolve_audience(
    channel: &Channel,
    actor: Entity,
    inroom: &Query<(Entity, &InRoom)>,
    rooms: &Query<&Room>,
) -> Vec<Entity> {
    match channel.scope {
        Scope::Room => {
            let Ok(actor_room) = inroom.get(actor).map(|(_, ir)| ir.room) else {
                return Vec::new();
            };
            in_room(actor_room, inroom.iter().map(|(e, ir)| (e, ir.room)), None)
        }
        Scope::Area => {
            let Ok(actor_room) = inroom.get(actor).map(|(_, ir)| ir.room) else {
                return Vec::new();
            };
            let Ok(actor_area) = rooms.get(actor_room).map(|r| r.area) else {
                return Vec::new();
            };
            inroom
                .iter()
                .filter(|(_, ir)| rooms.get(ir.room).is_ok_and(|r| r.area == actor_area))
                .map(|(e, _)| e)
                .collect()
        }
        Scope::Global => inroom.iter().map(|(e, _)| e).collect(),
    }
}

/// Audience from a pre-mapped `(Entity, room)` listing (for call sites whose
/// query carries extra columns). Room identity and area membership arrive as
/// closures so the caller keeps its own `Room` tuple shape.
pub fn resolve_audience_mapped(
    scope: Scope,
    actor_room: Option<Entity>,
    occupants: impl IntoIterator<Item = (Entity, Entity)>,
    mut same_area: impl FnMut(Entity, Entity) -> bool,
) -> Vec<Entity> {
    match scope {
        Scope::Room => match actor_room {
            Some(room) => in_room(room, occupants, None),
            None => Vec::new(),
        },
        Scope::Area => match actor_room {
            Some(room) => occupants
                .into_iter()
                .filter(|(_, r)| *r == room || same_area(room, *r))
                .map(|(e, _)| e)
                .collect(),
            None => Vec::new(),
        },
        Scope::Global => occupants.into_iter().map(|(e, _)| e).collect(),
    }
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
