use crate::components::{InRoom, Name, Room};
use crate::events::{Command, EngineCommand, InfoMessage, OocEvent, SayEvent, YellEvent};
use bevy::prelude::*;

/// Handles `say` commands, emitting a `SayEvent` for room broadcast and an
/// `InfoMessage` echo back to the speaker.
pub struct SocialPlugin;

impl Plugin for SocialPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EngineCommand>()
            .add_message::<InfoMessage>()
            .add_message::<SayEvent>()
            .add_message::<YellEvent>()
            .add_message::<OocEvent>()
            .add_systems(Update, (handle_say, handle_yell, handle_ooc));
    }
}

/// `say <text>`: broadcast to the room and echo "You say, '<text>'" to the actor.
fn handle_say(
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
            text: format!("You say, '{}'\r\n", text),
        });
    }
}

fn handle_yell(
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
            text: format!("You yell, '{}'\r\n", text),
        });
    }
}

fn handle_ooc(
    mut engine: MessageReader<EngineCommand>,
    mut ooc: MessageWriter<OocEvent>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Ooc { text } = &cmd.command else {
            continue;
        };
        ooc.write(OocEvent {
            actor: cmd.client,
            text: text.clone(),
        });
        info.write(InfoMessage {
            target: cmd.client,
            text: format!("[OOC] You: {}\r\n", text),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{InRoom, Name};
    use crate::events::{Command, EngineCommand, InfoMessage, SayEvent};

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(SocialPlugin);
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
            assert_eq!(ev.text, "You say, 'hi'\r\n");
            assert!(iter.next().is_none(), "expected exactly one InfoMessage");
        }
    }
}
