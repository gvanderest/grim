//! Quit event.

use bevy::prelude::*;

/// An actor wants to quit.
#[derive(Message, Debug)]
pub struct QuitIntent {
    pub actor: Entity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_intent_creates_correct_event() {
        let _intent = QuitIntent {
            actor: Entity::from_raw_u32(42).unwrap(),
        };
    }

    #[test]
    fn quit_intent_implements_message() {
        fn assert_message<T: bevy::prelude::Message>() {}
        assert_message::<QuitIntent>();
    }

    #[test]
    fn quit_intent_implements_debug() {
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<QuitIntent>();
    }
}
