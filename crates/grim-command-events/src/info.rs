//! Information/query events.

use bevy::prelude::*;

/// An actor wants to see the WHO list.
#[derive(Message, Debug)]
pub struct WhoIntent {
    pub actor: Entity,
}

/// An actor wants to see who's in their area.
#[derive(Message, Debug)]
pub struct WhereIntent {
    pub actor: Entity,
}

/// An actor wants to see the commands list.
#[derive(Message, Debug)]
pub struct CommandsIntent {
    pub actor: Entity,
}

/// An actor wants to see the areas list.
#[derive(Message, Debug)]
pub struct AreasIntent {
    pub actor: Entity,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_intent_creates_correct_event() {
        let _intent = WhereIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
        };
    }

    #[test]
    fn commands_intent_creates_correct_event() {
        let _intent = CommandsIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
        };
    }

    #[test]
    fn areas_intent_creates_correct_event() {
        let _intent = AreasIntent {
            actor: Entity::from_raw_u32(1).unwrap(),
        };
    }

    #[test]
    fn all_info_intents_implement_message() {
        fn assert_message<T: bevy::prelude::Message>() {}
        assert_message::<WhoIntent>();
        assert_message::<WhereIntent>();
        assert_message::<CommandsIntent>();
        assert_message::<AreasIntent>();
    }

    #[test]
    fn all_info_intents_implement_debug() {
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<WhoIntent>();
        assert_debug::<WhereIntent>();
        assert_debug::<CommandsIntent>();
        assert_debug::<AreasIntent>();
    }
}
