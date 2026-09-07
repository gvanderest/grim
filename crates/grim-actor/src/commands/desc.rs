//! The `desc` command: view and edit your own description paragraphs.
//!
//! Bare `desc` shows your paragraphs via a [`LookEntity`] on yourself, so the
//! rendering matches `look <name>` exactly. `desc clear` empties them,
//! `desc + <line>` appends one, `desc -` drops the last, and `desc edit`
//! opens the line editor preloaded with the paragraphs (applied back on
//! `@save` via the [`EditorDone`] callback).

use bevy::prelude::*;
use grim_core::components::Description;
use grim_core::events::{
    Command, DescOp, EditorDone, EditorKind, EngineCommand, InfoMessage, LookEntity, OpenEditor,
};
use grim_text::tr;
/// `desc …`: show or mutate the actor's own [`Description`] paragraphs.
/// Added lines are player data but never echoed back, so there is nothing to
/// escape. An actor with no `Description` reads as empty; the first `add`
/// inserts the component.
pub(crate) fn handle_desc(
    mut engine: MessageReader<EngineCommand>,
    mut descriptions: Query<&mut Description>,
    mut look_entity: MessageWriter<LookEntity>,
    mut info: MessageWriter<InfoMessage>,
    mut commands: Commands,
    mut open_editor: MessageWriter<OpenEditor>,
) {
    for cmd in engine.read() {
        let Command::Desc { op } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let reply = match op {
            DescOp::Show => {
                look_entity.write(LookEntity {
                    target: actor,
                    subject: actor,
                });
                continue;
            }
            DescOp::Clear => {
                if let Ok(mut desc) = descriptions.get_mut(actor) {
                    desc.0.clear();
                }
                tr!("desc.cleared")
            }
            DescOp::Add(line) => {
                match descriptions.get_mut(actor) {
                    Ok(mut desc) => desc.0.push(line.clone()),
                    Err(_) => {
                        commands
                            .entity(actor)
                            .insert(Description(vec![line.clone()]));
                    }
                }
                tr!("desc.added")
            }
            DescOp::Remove => {
                let removed = descriptions
                    .get_mut(actor)
                    .ok()
                    .and_then(|mut desc| desc.0.pop())
                    .is_some();
                if removed {
                    tr!("desc.removed")
                } else {
                    tr!("desc.empty")
                }
            }
            DescOp::Edit => {
                // Preload the editor with the current paragraphs (or nothing)
                // and let the completion event do the applying — no reply
                // here; the entry view is the response.
                let initial = descriptions
                    .get(actor)
                    .map(|desc| desc.0.clone())
                    .unwrap_or_default();
                open_editor.write(OpenEditor {
                    character: actor,
                    kind: EditorKind::Description,
                    initial,
                });
                continue;
            }
        };
        info.write(InfoMessage {
            target: actor,
            text: reply,
        });
    }
}

/// The editor callback for `desc edit`: apply a saved buffer to the actor's
/// own [`Description`] (inserting the component when missing), or confirm
/// the discard on `@exit`. Other [`EditorKind`]s are not ours.
pub(crate) fn handle_editor_done(
    mut done: MessageReader<EditorDone>,
    mut descriptions: Query<&mut Description>,
    mut commands: Commands,
    mut info: MessageWriter<InfoMessage>,
) {
    for finished in done.read() {
        match &finished.kind {
            EditorKind::Description => {
                let actor = finished.character;
                match &finished.lines {
                    Some(lines) => {
                        match descriptions.get_mut(actor) {
                            Ok(mut desc) => desc.0 = lines.clone(),
                            Err(_) => {
                                commands.entity(actor).insert(Description(lines.clone()));
                            }
                        }
                        info.write(InfoMessage {
                            target: actor,
                            text: tr!("desc.saved"),
                        });
                    }
                    None => {
                        info.write(InfoMessage {
                            target: actor,
                            text: tr!("desc.edit_cancelled"),
                        });
                    }
                }
            }
        }
    }
}

/// Wire the `desc` handler and the input/delivery messages it owns. The
/// world-happening event it emits (`LookEntity`) is registered by
/// `grim_world::WorldPlugin`.
pub(crate) fn register(app: &mut App) {
    app.add_message::<EngineCommand>()
        .add_message::<InfoMessage>()
        .add_message::<OpenEditor>()
        .add_message::<EditorDone>()
        .add_systems(Update, (handle_desc, handle_editor_done));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(grim_world::WorldPlugin);
        register(&mut app);
        app
    }

    fn send_desc(app: &mut App, actor: Entity, op: DescOp) {
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Desc { op },
        });
        app.update();
    }

    fn info_texts(app: &App) -> Vec<String> {
        let messages = app.world().resource::<Messages<InfoMessage>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|m| m.text.clone()).collect()
    }

    #[test]
    fn desc_show_emits_look_entity_on_self() {
        let mut app = test_app();
        let actor = app
            .world_mut()
            .spawn(Description(vec!["First.".into(), "Second.".into()]))
            .id();
        send_desc(&mut app, actor, DescOp::Show);
        let messages = app.world().resource::<Messages<LookEntity>>();
        let mut cursor = messages.get_cursor();
        let mut iter = cursor.read(messages);
        let ev = iter.next().expect("expected one LookEntity");
        assert_eq!(ev.target, actor);
        assert_eq!(ev.subject, actor);
        assert!(iter.next().is_none(), "expected exactly one LookEntity");
        assert!(info_texts(&app).is_empty());
    }

    #[test]
    fn desc_clear_empties_paragraphs() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(Description(vec!["Old.".into()])).id();
        send_desc(&mut app, actor, DescOp::Clear);
        assert!(app.world().get::<Description>(actor).unwrap().0.is_empty());
        assert_eq!(
            info_texts(&app),
            vec!["Your description has been cleared.\n"]
        );
    }

    #[test]
    fn desc_add_appends_and_inserts_when_missing() {
        let mut app = test_app();
        let actor = app
            .world_mut()
            .spawn(Description(vec!["First.".into()]))
            .id();
        let bare = app.world_mut().spawn(()).id();
        send_desc(&mut app, actor, DescOp::Add("Second.".into()));
        send_desc(&mut app, bare, DescOp::Add("Only.".into()));
        // `Commands` inserts flush on the next tick's apply.
        app.update();
        assert_eq!(
            app.world().get::<Description>(actor).unwrap().0,
            vec!["First.".to_string(), "Second.".to_string()]
        );
        assert_eq!(
            app.world().get::<Description>(bare).unwrap().0,
            vec!["Only.".to_string()]
        );
        assert_eq!(
            info_texts(&app),
            vec![
                "Line added to your description.\n",
                "Line added to your description.\n",
            ]
        );
    }

    #[test]
    fn desc_remove_pops_last_or_reports_empty() {
        let mut app = test_app();
        let actor = app
            .world_mut()
            .spawn(Description(vec!["First.".into(), "Second.".into()]))
            .id();
        let empty = app.world_mut().spawn(Description(Vec::new())).id();
        send_desc(&mut app, actor, DescOp::Remove);
        send_desc(&mut app, empty, DescOp::Remove);
        send_desc(&mut app, Entity::PLACEHOLDER, DescOp::Remove);
        assert_eq!(
            app.world().get::<Description>(actor).unwrap().0,
            vec!["First.".to_string()]
        );
        assert_eq!(
            info_texts(&app),
            vec![
                "Last line removed from your description.\n",
                "Your description is already empty.\n",
                "Your description is already empty.\n",
            ]
        );
    }

    #[test]
    fn desc_edit_opens_editor_preloaded_with_paragraphs() {
        let mut app = test_app();
        let actor = app
            .world_mut()
            .spawn(Description(vec!["First.".into(), "Second.".into()]))
            .id();
        send_desc(&mut app, actor, DescOp::Edit);

        let messages = app.world().resource::<Messages<OpenEditor>>();
        let mut cursor = messages.get_cursor();
        let mut iter = cursor.read(messages);
        let req = iter.next().expect("expected one OpenEditor");
        assert_eq!(req.character, actor);
        assert_eq!(req.kind, EditorKind::Description);
        assert_eq!(req.initial, vec!["First.".to_string(), "Second.".into()]);
        assert!(iter.next().is_none(), "expected exactly one OpenEditor");
        // The entry view is the response — no info line.
        assert!(info_texts(&app).is_empty());
    }

    #[test]
    fn desc_edit_with_no_description_opens_empty() {
        let mut app = test_app();
        let actor = app.world_mut().spawn_empty().id();
        send_desc(&mut app, actor, DescOp::Edit);

        let messages = app.world().resource::<Messages<OpenEditor>>();
        let mut cursor = messages.get_cursor();
        let req = cursor.read(messages).next().unwrap();
        assert!(req.initial.is_empty());
    }

    #[test]
    fn editor_save_replaces_description_and_confirms() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(Description(vec!["Old.".into()])).id();
        app.world_mut().write_message(EditorDone {
            character: actor,
            kind: EditorKind::Description,
            lines: Some(vec!["New one.".into(), "New two.".into()]),
        });
        app.update();

        assert_eq!(
            app.world().get::<Description>(actor).unwrap().0,
            vec!["New one.".to_string(), "New two.".into()]
        );
        assert_eq!(info_texts(&app), vec!["Your description has been saved.\n"]);
    }

    #[test]
    fn editor_save_inserts_when_missing() {
        let mut app = test_app();
        let actor = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(EditorDone {
            character: actor,
            kind: EditorKind::Description,
            lines: Some(vec!["Only.".into()]),
        });
        app.update();

        assert_eq!(
            app.world().get::<Description>(actor).unwrap().0,
            vec!["Only.".to_string()]
        );
    }

    #[test]
    fn editor_exit_discards_and_confirms() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(Description(vec!["Old.".into()])).id();
        app.world_mut().write_message(EditorDone {
            character: actor,
            kind: EditorKind::Description,
            lines: None,
        });
        app.update();

        assert_eq!(
            app.world().get::<Description>(actor).unwrap().0,
            vec!["Old.".to_string()]
        );
        assert_eq!(
            info_texts(&app),
            vec!["Edit cancelled, description unchanged.\n"]
        );
    }

    #[test]
    fn desc_ignores_other_commands() {
        let mut app = test_app();
        let actor = app.world_mut().spawn(()).id();
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Who,
        });
        app.update();
        assert!(info_texts(&app).is_empty());
        let messages = app.world().resource::<Messages<LookEntity>>();
        let mut cursor = messages.get_cursor();
        assert_eq!(cursor.read(messages).count(), 0);
    }
}
