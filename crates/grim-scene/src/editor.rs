//! The line-editor modal: a reusable multi-line input mode with a callback.
//!
//! A session enters the editor when engine code emits [`OpenEditor`]: this
//! module attaches an [`EditorSession`] to the session entity, and from then
//! on [`handle_ingame_input`](crate::input::handle_ingame_input) routes that
//! session's lines through [`handle_editor_line`] instead of the command
//! parser. Plain lines append to the buffer; `@save` / `@exit` / `@clear`
//! control the session. Closing emits [`EditorDone`], which the opener
//! consumes by [`EditorKind`] — that enum is the callback: new consumers add
//! a variant, never a new event type.
//!
//! This is the minimal special-case #75 anticipates, not the deferred
//! `grim-editor` scene: the session keeps its `InGame` state (world output
//! still arrives — pass-through policy), and a disconnect simply drops the
//! session entity with its unsaved buffer.

use bevy::prelude::*;
use grim_core::components::Client;
use grim_core::events::{EditorDone, EditorKind, OpenEditor};
use grim_networking::ConnectionOutput;
use grim_text::tr;

/// Modal editing state on a session entity (alongside [`Client`]). Present
/// exactly while the session is inside the editor.
#[derive(Component, Debug)]
pub struct EditorSession {
    pub character: Entity,
    pub kind: EditorKind,
    pub buffer: Vec<String>,
}

/// What one editor line does. Pure data so the line logic unit-tests without
/// an `App`; the input router applies it (writes messages, drops the session).
#[derive(Debug, PartialEq)]
pub(crate) enum EditorAction {
    /// Stay in the editor; `reply` (if any) goes straight to the connection.
    Stay { reply: Option<String> },
    /// Leave the editor; `done` goes to the opener (`lines: Some` on `@save`,
    /// `None` on `@exit`).
    Closed { done: EditorDone },
}

/// One line inside the editor: `@save` / `@exit` close, `@clear` empties,
/// `@`-anything-else hints, and plain lines append silently.
pub(crate) fn handle_editor_line(editor: &mut EditorSession, text: &str) -> EditorAction {
    let trimmed = text.trim();
    if let Some(command) = trimmed.strip_prefix('@') {
        match command.trim().to_lowercase().as_str() {
            "save" => {
                return EditorAction::Closed {
                    done: EditorDone {
                        character: editor.character,
                        kind: editor.kind.clone(),
                        lines: Some(std::mem::take(&mut editor.buffer)),
                    },
                };
            }
            "exit" => {
                editor.buffer.clear();
                return EditorAction::Closed {
                    done: EditorDone {
                        character: editor.character,
                        kind: editor.kind.clone(),
                        lines: None,
                    },
                };
            }
            "clear" => {
                editor.buffer.clear();
                return EditorAction::Stay {
                    reply: Some(tr!("editor.cleared")),
                };
            }
            _ => {
                return EditorAction::Stay {
                    reply: Some(tr!("editor.unknown")),
                };
            }
        }
    }
    editor.buffer.push(text.to_string());
    EditorAction::Stay { reply: None }
}

/// Engine requests: attach the modal session and show the entry view (current
/// lines numbered, plus the `@` help footer). A character with no session in
/// the world is skipped — nothing could read the editor anyway.
pub(crate) fn open_editor(
    mut requests: MessageReader<OpenEditor>,
    clients: Query<(Entity, &Client)>,
    mut commands: Commands,
    mut outputs: MessageWriter<ConnectionOutput>,
) {
    for req in requests.read() {
        let Some((session, client)) = clients
            .iter()
            .find(|(_, c)| c.character == Some(req.character))
        else {
            continue;
        };
        commands.entity(session).insert(EditorSession {
            character: req.character,
            kind: req.kind.clone(),
            buffer: req.initial.clone(),
        });
        let mut text = tr!("editor.enter.header");
        if req.initial.is_empty() {
            text.push_str(&tr!("editor.enter.empty"));
        } else {
            for (i, line) in req.initial.iter().enumerate() {
                let num = (i + 1).to_string();
                text.push_str(&tr!("editor.enter.row", num = num, line = line));
            }
        }
        text.push_str(&tr!("editor.enter.help"));
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(client.connection, text)
        });
    }
}

/// Wire the editor request reader. [`EditorDone`] is read by the opener.
pub(crate) fn register(app: &mut App) {
    app.add_message::<OpenEditor>()
        .add_message::<EditorDone>()
        .add_systems(Update, open_editor);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> EditorSession {
        EditorSession {
            character: Entity::PLACEHOLDER,
            kind: EditorKind::Description,
            buffer: vec!["Kept.".into()],
        }
    }

    #[test]
    fn plain_lines_append_silently() {
        let mut editor = session();
        assert_eq!(
            handle_editor_line(&mut editor, "Typed one."),
            EditorAction::Stay { reply: None }
        );
        assert_eq!(
            editor.buffer,
            vec!["Kept.".to_string(), "Typed one.".into()]
        );
    }

    #[test]
    fn save_closes_with_the_whole_buffer() {
        let mut editor = session();
        handle_editor_line(&mut editor, "Typed one.");
        assert_eq!(
            handle_editor_line(&mut editor, "@save"),
            EditorAction::Closed {
                done: EditorDone {
                    character: Entity::PLACEHOLDER,
                    kind: EditorKind::Description,
                    lines: Some(vec!["Kept.".into(), "Typed one.".into()]),
                },
            }
        );
        assert!(editor.buffer.is_empty());
    }

    #[test]
    fn save_is_case_insensitive_and_trims() {
        let mut editor = session();
        let action = handle_editor_line(&mut editor, "  @SAVE  ");
        assert!(matches!(action, EditorAction::Closed { .. }));
    }

    #[test]
    fn exit_discards_and_closes() {
        let mut editor = session();
        assert_eq!(
            handle_editor_line(&mut editor, "@exit"),
            EditorAction::Closed {
                done: EditorDone {
                    character: Entity::PLACEHOLDER,
                    kind: EditorKind::Description,
                    lines: None,
                },
            }
        );
        assert!(editor.buffer.is_empty());
    }

    #[test]
    fn clear_empties_and_stays() {
        let mut editor = session();
        assert_eq!(
            handle_editor_line(&mut editor, "@clear"),
            EditorAction::Stay {
                reply: Some("Buffer cleared.\n".into())
            }
        );
        assert!(editor.buffer.is_empty());
    }

    #[test]
    fn unknown_at_command_hints_and_stays() {
        let mut editor = session();
        assert_eq!(
            handle_editor_line(&mut editor, "@frobnicate"),
            EditorAction::Stay {
                reply: Some("Unknown editor command. Use @save, @exit, or @clear.\n".into())
            }
        );
        assert_eq!(editor.buffer, vec!["Kept.".to_string()]);
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<ConnectionOutput>();
        register(&mut app);
        app
    }

    fn session_with_client(app: &mut App, character: Option<Entity>) -> (Entity, Entity) {
        let conn = app.world_mut().spawn_empty().id();
        let session = app
            .world_mut()
            .spawn(Client {
                connection: conn,
                state: grim_core::components::ClientState::InGame,
                account: None,
                character,
                input_queue: std::collections::VecDeque::new(),
                command_cooldown: Timer::from_seconds(0.5, TimerMode::Once),
                last_input: None,
            })
            .id();
        (session, conn)
    }

    fn outputs(app: &App) -> Vec<(Entity, String)> {
        let messages = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = messages.get_cursor();
        cursor
            .read(messages)
            .map(|o| (o.connection, o.text.clone()))
            .collect()
    }

    #[test]
    fn open_editor_attaches_session_and_shows_numbered_lines() {
        let mut app = test_app();
        let actor = app.world_mut().spawn_empty().id();
        let (session, conn) = session_with_client(&mut app, Some(actor));
        app.world_mut().write_message(OpenEditor {
            character: actor,
            kind: EditorKind::Description,
            initial: vec!["First.".into(), "Second.".into()],
        });
        app.update();

        let editor = app.world().get::<EditorSession>(session).unwrap();
        assert_eq!(editor.buffer, vec!["First.".to_string(), "Second.".into()]);
        let out = outputs(&app);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, conn);
        assert!(out[0].1.contains("Editing description."));
        assert!(out[0].1.contains("1. First."));
        assert!(out[0].1.contains("2. Second."));
        assert!(out[0].1.contains("@save"));
    }

    #[test]
    fn open_editor_for_unknown_character_emits_nothing() {
        let mut app = test_app();
        let actor = app.world_mut().spawn_empty().id();
        session_with_client(&mut app, None);
        app.world_mut().write_message(OpenEditor {
            character: actor,
            kind: EditorKind::Description,
            initial: vec![],
        });
        app.update();

        assert!(outputs(&app).is_empty());
    }
}
