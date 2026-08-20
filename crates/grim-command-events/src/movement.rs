//! Movement events.

use bevy::prelude::*;

use crate::Cardinal;

/// An actor wants to move in a direction.
#[derive(Message, Debug)]
pub struct MoveIntent {
    pub actor: Entity,
    pub direction: Cardinal,
}

/// An actor wants to look at something (room or entity).
#[derive(Message, Debug)]
pub struct LookIntent {
    pub actor: Entity,
    pub target: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_intent_creates_correct_event() {
        let entity = Entity::from_raw_u32(42).unwrap();
        let intent = MoveIntent {
            actor: entity,
            direction: Cardinal::North,
        };
        assert_eq!(intent.actor, entity);
        assert_eq!(intent.direction, Cardinal::North);
    }

    #[test]
    fn look_intent_with_target() {
        let intent = LookIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
            target: Some("chest".to_string()),
        };
        assert_eq!(intent.target, Some("chest".to_string()));
    }

    #[test]
    fn look_intent_without_target() {
        let intent = LookIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
            target: None,
        };
        assert_eq!(intent.target, None);
    }

    #[test]
    fn all_movement_intents_implement_message() {
        fn assert_message<T: bevy::prelude::Message>() {}
        assert_message::<MoveIntent>();
        assert_message::<LookIntent>();
    }

    #[test]
    fn all_movement_intents_implement_debug() {
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<MoveIntent>();
        assert_debug::<LookIntent>();
    }
}
