//! Channel registry: stores channel configurations and resolves audience.

use std::collections::HashMap;

use bevy::prelude::*;

use grim_actor::in_room;
use grim_core::channel::{Channel, Scope};

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

/// Audience from a pre-mapped `(Entity, room)` listing (for call sites whose
/// query carries extra columns). Room scope funnels through the shared
/// [`in_room`] helper; the area arm walks [`Room::area`] (ADR-0001) via the
/// `same_area` closure, failing closed when either `Room` row is missing.
/// This is the one audience entry point: `grim-scene::emit_channel` delegates
/// here (mapped from its rich tuple), keeping eligibility + rendering local
/// per ADR-0005.
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
            // Fail closed like `resolve_audience`: same-room reaches only via
            // `same_area` (which requires both `Room` rows to resolve), never
            // by bare room identity.
            Some(room) => occupants
                .into_iter()
                .filter(|(_, r)| same_area(room, *r))
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

    fn same_area_map(pairs: &[(Entity, Entity)]) -> impl FnMut(Entity, Entity) -> bool + '_ {
        move |a, b| {
            let area_of = |r: Entity| pairs.iter().find(|(e, _)| *e == r).map(|(_, a)| *a);
            area_of(a).zip(area_of(b)).is_some_and(|(x, y)| x == y)
        }
    }

    #[test]
    fn audience_room_lists_only_the_room() {
        let mut app = App::new();
        let here = app.world_mut().spawn_empty().id();
        let there = app.world_mut().spawn_empty().id();
        let a = app.world_mut().spawn_empty().id();
        let b = app.world_mut().spawn_empty().id();
        let c = app.world_mut().spawn_empty().id();
        let occupants = vec![(a, here), (b, here), (c, there)];
        let mut got = resolve_audience_mapped(Scope::Room, Some(here), occupants, |_, _| false);
        got.sort();
        let mut want = vec![a, b];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn audience_room_without_actor_is_empty() {
        let a = Entity::PLACEHOLDER;
        let got =
            resolve_audience_mapped(Scope::Room, None, vec![(a, Entity::PLACEHOLDER)], |_, _| {
                panic!("no area lookup without an actor room")
            });
        assert!(got.is_empty());
    }

    #[test]
    fn audience_area_spans_rooms_fails_closed() {
        let mut app = App::new();
        let r1 = app.world_mut().spawn_empty().id();
        let r2 = app.world_mut().spawn_empty().id();
        let r3 = app.world_mut().spawn_empty().id();
        let area_a = app.world_mut().spawn_empty().id();
        let area_b = app.world_mut().spawn_empty().id();
        let missing = app.world_mut().spawn_empty().id();
        // (room, area); `missing` has no area row.
        let areas = vec![(r1, area_a), (r2, area_a), (r3, area_b)];
        let a = app.world_mut().spawn_empty().id();
        let b = app.world_mut().spawn_empty().id();
        let c = app.world_mut().spawn_empty().id();
        let d = app.world_mut().spawn_empty().id();
        let occupants = vec![(a, r1), (b, r2), (c, r3), (d, missing)];
        let mut got =
            resolve_audience_mapped(Scope::Area, Some(r1), occupants, same_area_map(&areas));
        got.sort();
        let mut want = vec![a, b];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn audience_global_lists_everyone() {
        let mut app = App::new();
        let a = app.world_mut().spawn_empty().id();
        let b = app.world_mut().spawn_empty().id();
        let mut got = resolve_audience_mapped(
            Scope::Global,
            None,
            vec![(a, Entity::PLACEHOLDER), (b, Entity::PLACEHOLDER)],
            |_, _| panic!("no area lookup on global scope"),
        );
        got.sort();
        let mut want = vec![a, b];
        want.sort();
        assert_eq!(got, want);
    }
}
