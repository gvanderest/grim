//! Social/channel events.

use bevy::prelude::*;

/// An actor wants to say something (room-scoped).
#[derive(Message, Debug)]
pub struct SayIntent {
    pub actor: Entity,
    pub text: String,
}

/// An actor wants to yell something (area-scoped).
#[derive(Message, Debug)]
pub struct YellIntent {
    pub actor: Entity,
    pub text: String,
}

/// An actor wants to say something OOC (global).
#[derive(Message, Debug)]
pub struct OocIntent {
    pub actor: Entity,
    pub text: String,
}

/// An actor wants to whisper to another player.
#[derive(Message, Debug)]
pub struct TellIntent {
    pub actor: Entity,
    pub target: String,
    pub text: String,
}

/// An actor wants to reply to the last whisper sender.
#[derive(Message, Debug)]
pub struct ReplyIntent {
    pub actor: Entity,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn say_intent_contains_text() {
        let intent = SayIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
            text: "Hello, world!".to_string(),
        };
        assert_eq!(intent.text, "Hello, world!");
    }

    #[test]
    fn yell_intent_contains_text() {
        let intent = YellIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
            text: "HELLO!".to_string(),
        };
        assert_eq!(intent.text, "HELLO!");
    }

    #[test]
    fn ooc_intent_contains_text() {
        let intent = OocIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
            text: "OOC message".to_string(),
        };
        assert_eq!(intent.text, "OOC message");
    }

    #[test]
    fn tell_intent_has_target_and_text() {
        let intent = TellIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
            target: "PlayerTwo".to_string(),
            text: "Hello there!".to_string(),
        };
        assert_eq!(intent.target, "PlayerTwo");
        assert_eq!(intent.text, "Hello there!");
    }

    #[test]
    fn reply_intent_has_text() {
        let intent = ReplyIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
            text: "Replying to you".to_string(),
        };
        assert_eq!(intent.text, "Replying to you");
    }

    #[test]
    fn all_social_intents_implement_message() {
        fn assert_message<T: bevy::prelude::Message>() {}
        assert_message::<SayIntent>();
        assert_message::<YellIntent>();
        assert_message::<OocIntent>();
        assert_message::<TellIntent>();
        assert_message::<ReplyIntent>();
    }

    #[test]
    fn all_social_intents_implement_debug() {
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<SayIntent>();
        assert_debug::<YellIntent>();
        assert_debug::<OocIntent>();
        assert_debug::<TellIntent>();
        assert_debug::<ReplyIntent>();
    }
}
