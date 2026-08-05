//! `look` rendering: turn a `look` command into the room/entity description
//! events (`LookRoom`/`LookEntity`) or a "not here" info message.

use bevy::prelude::*;
use grim_engine_types::components::{InRoom, Name};
use grim_engine_types::events::{Command, EngineCommand, InfoMessage, LookEntity, LookRoom};

/// `look` / `look <target>`: show the actor's room or a named entity in it.
pub(crate) fn handle_look(
    mut engine: MessageReader<EngineCommand>,
    inroom: Query<&InRoom>,
    named: Query<(Entity, &InRoom, &Name)>,
    mut look_room: MessageWriter<LookRoom>,
    mut look_entity: MessageWriter<LookEntity>,
    mut info: MessageWriter<InfoMessage>,
) {
    for cmd in engine.read() {
        let Command::Look { target } = &cmd.command else {
            continue;
        };
        let actor = cmd.client;
        let Ok(actor_room) = inroom.get(actor) else {
            continue;
        };
        match target {
            None => {
                look_room.write(LookRoom {
                    target: actor,
                    room: actor_room.room,
                });
            }
            Some(name) => {
                let want = name.to_lowercase();
                let room = actor_room.room;
                let subject = named
                    .iter()
                    .find(|(_, ir, nm)| ir.room == room && nm.0.to_lowercase() == want)
                    .map(|(e, _, _)| e);
                match subject {
                    Some(subject) => {
                        look_entity.write(LookEntity {
                            target: actor,
                            subject,
                        });
                    }
                    None => {
                        info.write(InfoMessage {
                            target: actor,
                            text: "You don't see that here.\n".into(),
                        });
                    }
                }
            }
        }
    }
}
