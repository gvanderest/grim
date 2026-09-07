//! Room-transition events: observe-only attempt notifications plus committed
//! facts.
//!
//! A `move`/`goto` that resolves to a real destination emits, in order:
//! `AttemptLeave` (the room being left) and `AttemptEnter` (the room being
//! entered) *before* the actor is placed, then the committed [`Leave`] and
//! [`Enter`] facts after. The `Attempt*` pair is observe-only today — nothing
//! can deny a move yet — so scripts (or future guards) can react to intent
//! separately from the outcome without any veto power being implied.

use bevy::prelude::*;

/// A being is about to leave `room`. Observe-only: emitted before placement,
/// denies nothing.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptLeave {
    /// The being that is moving.
    pub actor: Entity,
    /// The room it is leaving.
    pub room: Entity,
}

/// A being is about to enter `room`. Observe-only: emitted before placement,
/// denies nothing.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptEnter {
    /// The being that is moving.
    pub actor: Entity,
    /// The room it is entering.
    pub room: Entity,
}

/// A being has left `room`. Committed: the actor is already placed elsewhere.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leave {
    /// The being that moved.
    pub actor: Entity,
    /// The room it left.
    pub room: Entity,
}

/// A being has entered `room`. Committed: the actor is already placed there.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Enter {
    /// The being that moved.
    pub actor: Entity,
    /// The room it entered.
    pub room: Entity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_events_carry_actor_and_room() {
        let actor = Entity::from_raw_u32(1).unwrap();
        let room = Entity::from_raw_u32(2).unwrap();
        assert_eq!(AttemptLeave { actor, room }.room, room);
        assert_eq!(AttemptEnter { actor, room }.actor, actor);
        assert_eq!(Leave { actor, room }.room, room);
        assert_eq!(Enter { actor, room }.actor, actor);
    }
}
