//! Admin events.

use bevy::prelude::*;

/// An admin wants to schedule a shutdown.
#[derive(Message, Debug)]
pub struct ShutdownIntent {
    pub actor: Entity,
    pub seconds: u64,
}

/// An admin wants to teleport to a room.
#[derive(Message, Debug)]
pub struct GotoIntent {
    pub actor: Entity,
    pub target: String,
}

/// An admin wants to broadcast to all players.
#[derive(Message, Debug)]
pub struct GechoIntent {
    pub actor: Entity,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_intent_with_seconds() {
        let intent = ShutdownIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
            seconds: 30,
        };
        assert_eq!(intent.seconds, 30);
    }

    #[test]
    fn goto_intent_has_target() {
        let intent = GotoIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
            target: "room_123".to_string(),
        };
        assert_eq!(intent.target, "room_123");
    }

    #[test]
    fn gecho_intent_has_text() {
        let intent = GechoIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
            text: "Broadcast message".to_string(),
        };
        assert_eq!(intent.text, "Broadcast message");
    }

    #[test]
    fn all_admin_intents_implement_message() {
        fn assert_message<T: bevy::prelude::Message>() {}
        assert_message::<ShutdownIntent>();
        assert_message::<GotoIntent>();
        assert_message::<GechoIntent>();
    }

    #[test]
    fn all_admin_intents_implement_debug() {
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<ShutdownIntent>();
        assert_debug::<GotoIntent>();
        assert_debug::<GechoIntent>();
    }
}
