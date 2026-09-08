//! Room-transition events: synchronous attempt triggers plus committed facts.
//!
//! A `move` that resolves to a real destination runs phases, in order:
//!
//! 1. **Pre-checks** — a valid exit exists. Nothing fires otherwise.
//! 2. **`AttemptWalk`** — the walk intent itself, room-agnostic.
//! 3. **`AttemptLeave`** in the room being left, then **`AttemptEnter`** in
//!    the room being entered — every observer runs *synchronously*, before
//!    anything moves.
//! 4. **If no attempt was denied** — placement, the [`MoveEvent`] fact (the
//!    "walking happened" event the game acts on), and the arrival description.
//! 5. **Next tick** — the committed [`Leave`]/[`Enter`] facts. Facts fire a
//!    tick after placement so their speech lands in a later flush than the
//!    arrival: the room loads before the hello that reacts to it.
//!
//! Attempts are Bevy *trigger* events, not messages: the orchestrator fires
//! them with `trigger_ref`, so observers cannot run late — script speech from
//! an attempt is written before the arrival description, and renders first.
//! Denial is a monotonic latch: observers set `denied` (never clear it), a
//! denied move never places, and observer order only decides whose echo the
//! scripter already spoke. Echo on denial belongs to the denier — a script
//! `say`s its refusal before calling `deny()` — so the engine itself stays
//! silent.

use bevy::prelude::*;
use grim_core::cardinal::Cardinal;

/// A being is about to walk `direction` from `room`. The first phase of a
/// walk: synchronous and vetoable, room-agnostic — a "hold person" stops the
/// walk itself, while room guards deny the `AttemptLeave`/`AttemptEnter`
/// that follow.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptWalk {
    /// The being that is moving.
    pub actor: Entity,
    /// The room it is leaving.
    pub room: Entity,
    /// The direction it tries to walk.
    pub direction: Cardinal,
    /// Denial latch. Observers set, never clear; a denied move never places.
    pub denied: bool,
}

/// A being is about to leave `room`. Synchronous: every observer runs before
/// placement. Set `denied` to block the move.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptLeave {
    /// The being that is moving.
    pub actor: Entity,
    /// The room it is leaving.
    pub room: Entity,
    /// Denial latch. Observers set, never clear; a denied move never places.
    pub denied: bool,
}

/// A being is about to enter `room`. Synchronous: every observer runs before
/// placement. Set `denied` to block the move.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptEnter {
    /// The being that is moving.
    pub actor: Entity,
    /// The room it is entering.
    pub room: Entity,
    /// Denial latch. Observers set, never clear; a denied move never places.
    pub denied: bool,
}

/// A being has left `room`. Committed: the actor is already placed elsewhere.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leave {
    /// The being that moved.
    pub actor: Entity,
    /// The room it left.
    pub room: Entity,
}

/// A being has entered `room`. Committed: the actor is already placed there.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
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
    fn attempts_carry_a_denial_latch() {
        let actor = Entity::from_raw_u32(1).unwrap();
        let room = Entity::from_raw_u32(2).unwrap();
        let mut leave = AttemptLeave {
            actor,
            room,
            denied: false,
        };
        assert!(!leave.denied);
        leave.denied = true;
        assert_eq!((leave.actor, leave.room, leave.denied), (actor, room, true));
        let enter = AttemptEnter {
            actor,
            room,
            denied: false,
        };
        assert_eq!((enter.actor, enter.room), (actor, room));
    }

    #[test]
    fn facts_carry_actor_and_room() {
        let actor = Entity::from_raw_u32(1).unwrap();
        let room = Entity::from_raw_u32(2).unwrap();
        assert_eq!(Leave { actor, room }.room, room);
        assert_eq!(Enter { actor, room }.actor, actor);
    }
}
