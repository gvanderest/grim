//! Actor placement: which room an actor currently stands in.

use bevy::prelude::*;

/// Which room an entity is currently in. The counterpart to `grim_world`'s room
/// topology: the world owns the rooms, the actor layer owns who is in them.
#[derive(Component, Debug, Clone)]
pub struct InRoom {
    pub room: Entity,
}
