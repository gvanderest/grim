//! `ooc <text>`: out-of-character global chat.

use bevy::prelude::*;
use grim_core::events::{Command, EngineCommand, InfoMessage, OocEvent};

pub(crate) fn handle_ooc(
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
            text: format!("[OOC] You: {}\n", text),
        });
    }
}

/// Wire the `ooc` handler and the messages it reads/emits.
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_message::<OocEvent>()
        .add_systems(Update, handle_ooc);
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
    fn ooc_emits_ooc_event_and_info_message() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(()).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Ooc {
                text: "hello everyone".into(),
            },
        });
        app.update();
        {
            let messages = app.world().resource::<Messages<OocEvent>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one OocEvent");
            assert_eq!(ev.actor, actor);
            assert_eq!(ev.text, "hello everyone");
            assert!(iter.next().is_none(), "expected exactly one OocEvent");
        }
        {
            let messages = app.world().resource::<Messages<InfoMessage>>();
            let mut cursor = messages.get_cursor();
            let mut iter = cursor.read(messages);
            let ev = iter.next().expect("expected one InfoMessage");
            assert_eq!(ev.target, actor);
            assert_eq!(ev.text, "[OOC] You: hello everyone\n");
            assert!(iter.next().is_none(), "expected exactly one InfoMessage");
        }
    }
}
