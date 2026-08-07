//! `say <text>`: broadcast to the room and echo "You say, '<text>'" to the actor.

use grim_text::tr;

use bevy::prelude::*;
use grim_actor::InRoom;
use grim_core::components::Name;
use grim_core::events::{Command, EngineCommand, InfoMessage, SayEvent};

pub(crate) fn handle_say(
    mut engine: MessageReader<EngineCommand>,
    inroom: Query<(&InRoom, &Name)>,
    mut say: MessageWriter<SayEvent>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Say { text } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Ok((ir, _name)) = inroom.get(actor) else {
            continue;
        };
        say.write(SayEvent {
            room: ir.room,
            actor,
            text: text.clone(),
        });
        info.write(InfoMessage {
            target: actor,
            text: tr!("social.say.first_party", text = text),
        });
    }
}

/// Wire the `say` handler and the messages it reads/emits.
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_message::<SayEvent>()
        .add_systems(Update, handle_say);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        register(&mut app);
        app
    }

    #[test]
    fn say_emits_say_event_and_info_message() {
        let mut app = test_app();
        let room = app.world_mut().spawn(()).id();
        let actor = app
            .world_mut()
            .spawn((InRoom { room }, Name("hero".into())))
            .id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Say { text: "hi".into() },
        });
        app.update();
        {
            let messages = app.world().resource::<Messages<SayEvent>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one SayEvent");
            assert_eq!(ev.room, room);
            assert_eq!(ev.actor, actor);
            assert_eq!(ev.text, "hi");
            assert!(iter.next().is_none(), "expected exactly one SayEvent");
        }
        {
            let messages = app.world().resource::<Messages<InfoMessage>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one InfoMessage");
            assert_eq!(ev.target, actor);
            assert_eq!(ev.text, "@xf0fYou say @r'@x909hi@r'\n");
            assert!(iter.next().is_none(), "expected exactly one InfoMessage");
        }
    }

    #[test]
    fn test_say_without_inroom_does_nothing() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(()).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Say {
                text: "hello".into(),
            },
        });
        app.update();
        let messages = app.world().resource::<Messages<SayEvent>>();
        let mut cursor = messages.get_cursor();
        assert_eq!(cursor.read(messages).count(), 0, "No SayEvent expected");
        let info = app.world().resource::<Messages<InfoMessage>>();
        let mut cursor = info.get_cursor();
        assert_eq!(cursor.read(info).count(), 0, "No InfoMessage expected");
    }
}
