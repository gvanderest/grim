//! The stable, entity-independent storage location of a room.
//!
//! [`RoomLocation`] records a room by its area + room `friendly_id`s rather than
//! by entity, so it survives a world reseed. Persisting it (on
//! `grim_actor::Character.last_room`) lets a player be placed back into the *new*
//! instance of the same room after a restart or copyover.

use serde::{Deserialize, Serialize};

/// The stable, entity-independent storage location of a room: area + room
/// `friendly_id`s. Persisted on a character (`grim_actor::Character.last_room`)
/// so a player can be placed back into the *new* instance of the same room after
/// a restart or copyover.
///
/// Lives in `grim-world` (the being-free world layer) because it names world
/// topology, not the being that stores it. `grim-actor`, `grim-persistence`, and
/// `grim-scene` all depend on `grim-world`, so the location type sits below all
/// of them with no reverse edge.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RoomLocation {
    pub area: String,
    pub room: String,
}
