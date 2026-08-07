//! `yell <text>`: broadcast to every room in the actor's area.

use bevy::prelude::*;
use grim_actor::InRoom;
use grim_core::components::Name;
use grim_core::events::{Command, EngineCommand, InfoMessage, YellEvent};
use grim_world::Room;

pub(crate) fn handle_yell(
    mut engine: MessageReader<EngineCommand>,
    inroom: Query<(&InRoom, &Name)>,
    rooms: Query<&Room>,
    mut yell: MessageWriter<YellEvent>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Yell { text } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Ok((ir, _name)) = inroom.get(actor) else {
            continue;
        };
        let Ok(room) = rooms.get(ir.room) else {
            continue;
        };
        yell.write(YellEvent {
            area: room.area,
            actor,
            text: text.clone(),
        });
        info.write(InfoMessage {
            target: actor,
            text: format!("You yell, '{}'\n", text),
        });
    }
}

/// Wire the `yell` handler and the messages it reads/emits.
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_message::<YellEvent>()
        .add_systems(Update, handle_yell);
}

#[cfg(test)]
mod tests {
    use super::*;
    use grim_core::GrimId;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        register(&mut app);
        app
    }

    #[test]
    fn yell_emits_yell_event_and_info_message() {
        let mut app = test_app();
        let area = app.world_mut().spawn(()).id();
        let room = app
            .world_mut()
            .spawn(Room {
                id: GrimId::new(),
                friendly_id: "test".into(),
                name: "Test Room".into(),
                description: "".into(),
                area,
            })
            .id();
        let actor = app
            .world_mut()
            .spawn((InRoom { room }, Name("hero".into())))
            .id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Yell {
                text: "help!".into(),
            },
        });
        app.update();
        {
            let messages = app.world().resource::<Messages<YellEvent>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one YellEvent");
            assert_eq!(ev.area, area);
            assert_eq!(ev.actor, actor);
            assert_eq!(ev.text, "help!");
            assert!(iter.next().is_none(), "expected exactly one YellEvent");
        }
        {
            let messages = app.world().resource::<Messages<InfoMessage>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one InfoMessage");
            assert_eq!(ev.target, actor);
            assert_eq!(ev.text, "You yell, 'help!'\n");
            assert!(iter.next().is_none(), "expected exactly one InfoMessage");
        }
    }

    #[test]
    fn test_yell_without_inroom_does_nothing() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(()).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Yell {
                text: "help!".into(),
            },
        });
        app.update();
        let messages = app.world().resource::<Messages<YellEvent>>();
        let mut cursor = messages.get_cursor();
        assert_eq!(cursor.read(messages).count(), 0, "No YellEvent expected");
        let info = app.world().resource::<Messages<InfoMessage>>();
        let mut cursor = info.get_cursor();
        assert_eq!(cursor.read(info).count(), 0, "No InfoMessage expected");
    }
}
