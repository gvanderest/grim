//! The line-editor modal: a reusable multi-line input mode with a callback.
//!
//! The modal state is [`Client::editor`](grim_core::components::Client) — a
//! plain field, not a component — so opening and closing are visible to later
//! input lines in the same tick. (Deferred `Commands` could never do that: a
//! `desc edit` followed by a text line in one update would dispatch the line
//! as a command, and an `@save` followed by `look` would swallow the look.)
//!
//! Two ways in, one way out:
//!
//! - Parsed input (`desc edit`) opens synchronously in the input dispatch via
//!   [`open_inline`]: same-tick lines already route to the buffer.
//! - Programmatic requests arrive as [`OpenEditor`] and are attached by the
//!   [`open_editor`] system (no triggering input line exists there, so no
//!   same-tick race applies).
//! - Closing always emits [`EditorDone`], which the opener consumes by
//!   [`EditorKind`] — that enum is the callback: new consumers add a variant,
//!   never a new event type.
//!
//! This is the minimal special-case #75 anticipates, not the deferred
//! `grim-editor` scene: the session keeps its `InGame` state (world output
//! still arrives — pass-through policy), and a disconnect simply drops the
//! session with its unsaved buffer.

use bevy::prelude::*;
use grim_core::components::{Client, Description, EditorSession};
use grim_core::events::{EditorDone, EditorKind, OpenEditor};
use grim_networking::ConnectionOutput;
use grim_text::tr;

/// What one editor line does. Pure data so the line logic unit-tests without
/// an `App`; the input router applies it (writes messages, clears the field).
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

/// The entry view: header, current lines numbered (or the empty line), and
/// the `@` help footer.
pub(crate) fn entry_text(initial: &[String]) -> String {
    let mut text = tr!("editor.enter.header");
    if initial.is_empty() {
        text.push_str(&tr!("editor.enter.empty"));
    } else {
        for (i, line) in initial.iter().enumerate() {
            let num = (i + 1).to_string();
            text.push_str(&tr!("editor.enter.row", num = num, line = line));
        }
    }
    text.push_str(&tr!("editor.enter.help"));
    text
}

/// Synchronous open for parsed input: preload, set the field, show the entry
/// view. Runs inside the input dispatch so the very next line — even in the
/// same tick — already routes to the buffer.
pub(crate) fn open_inline(
    client: &mut Client,
    conn: Entity,
    character: Entity,
    kind: EditorKind,
    initial: Vec<String>,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let text = entry_text(&initial);
    client.editor = Some(EditorSession {
        character,
        kind,
        buffer: initial,
    });
    outputs.write(ConnectionOutput {
        echo: None,
        ..ConnectionOutput::new(conn, text)
    });
}

/// Synchronous `desc edit` open for the input dispatch: preload the actor's
/// paragraphs, set the field, show the entry view. Inline here (not via the
/// engine queue) so the editor is set before the next input line routes —
/// even in the same tick. The engine never sees this op.
pub(crate) fn open_desc_edit(
    client: &mut Client,
    conn: Entity,
    character: Entity,
    descriptions: &Query<&Description>,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let initial = preload(descriptions, character);
    open_inline(
        client,
        conn,
        character,
        EditorKind::Description,
        initial,
        outputs,
    );
}

/// Programmatic requests: attach the modal and show the entry view. A
/// character with no session in the world is skipped — nothing could read
/// the editor anyway.
pub(crate) fn open_editor(
    mut requests: MessageReader<OpenEditor>,
    mut clients: Query<(Entity, &mut Client)>,
    mut outputs: MessageWriter<ConnectionOutput>,
) {
    for req in requests.read() {
        let Some((_, mut client)) = clients
            .iter_mut()
            .find(|(_, c)| c.character == Some(req.character))
        else {
            continue;
        };
        let conn = client.connection;
        open_inline(
            &mut client,
            conn,
            req.character,
            req.kind.clone(),
            req.initial.clone(),
            &mut outputs,
        );
    }
}

/// Wire the editor request reader. [`EditorDone`] is read by the opener.
pub(crate) fn register(app: &mut App) {
    app.add_message::<OpenEditor>()
        .add_message::<EditorDone>()
        .add_systems(Update, open_editor);
}

/// Current [`Description`] paragraphs for the editor preload, empty when the
/// actor has none.
pub(crate) fn preload(descriptions: &Query<&Description>, actor: Entity) -> Vec<String> {
    descriptions
        .get(actor)
        .map(|desc| desc.0.clone())
        .unwrap_or_default()
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

    #[test]
    fn entry_text_numbers_lines() {
        let text = entry_text(&["First.".to_string(), "Second.".into()]);
        assert!(text.contains("Editing description."));
        assert!(text.contains("1. First."));
        assert!(text.contains("2. Second."));
        assert!(text.contains("@save"));
        assert!(entry_text(&[]).contains("(empty)"));
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
                editor: None,
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
    fn open_editor_attaches_field_and_shows_numbered_lines() {
        let mut app = test_app();
        let actor = app.world_mut().spawn_empty().id();
        let (session, conn) = session_with_client(&mut app, Some(actor));
        app.world_mut().write_message(OpenEditor {
            character: actor,
            kind: EditorKind::Description,
            initial: vec!["First.".into(), "Second.".into()],
        });
        app.update();

        let client = app.world().get::<Client>(session).unwrap();
        let editor = client.editor.as_ref().unwrap();
        assert_eq!(editor.buffer, vec!["First.".to_string(), "Second.".into()]);
        let out = outputs(&app);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, conn);
        assert!(out[0].1.contains("1. First."));
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
