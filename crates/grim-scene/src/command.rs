//! In-game command handling: parse a line for a session in `InGame`, answer the
//! session-local commands (who/where/commands/areas), gate admin-only commands,
//! and drain the per-client queue into engine commands under a cooldown.

use bevy::prelude::*;
use grim_engine_types::components::{
    Character, Client, ClientState, InRoom, Linkdead, Name as GrimName,
};
use grim_engine_types::events::{Command, EngineCommand, LogoutAnnounce};
use grim_networking::{ConnectionOutput, DisconnectRequest};
use grim_text::tr;

use crate::formatter;
use crate::params::{RoomResolver, SessionRes};
use crate::parser;

/// InGame: parse the line (honouring `!` repeat), answer session-local commands
/// directly, admin-gate shutdown/goto, and queue everything else for cooldown.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_ingame(
    client: &mut Client,
    conn: Entity,
    text: &str,
    characters: &Query<(Entity, &Character, &GrimName)>,
    player_chars: &Query<(Entity, &GrimName, &InRoom, Option<&Character>)>,
    linkdead: &Query<&Linkdead>,
    rooms: &RoomResolver,
    res: &SessionRes,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let Some(char_entity) = client.character else {
        return;
    };
    // Handle "!" to repeat last command
    let text_to_parse = if text.trim() == "!" {
        if let Some(ref last_input) = client.last_input {
            last_input.as_str()
        } else {
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(conn, "No previous command to repeat.\n")
            });
            return;
        }
    } else {
        text
    };

    if let Some(cmd) = parser::parse_command(&res.registry, text_to_parse) {
        // Update last_input for future "!" repeats (store only non-"!" input)
        client.last_input = Some(text_to_parse.to_string());
        // Handle special commands immediately
        match &cmd {
            Command::Who => {
                outputs.write(ConnectionOutput {
                    echo: None,
                    ..ConnectionOutput::new(conn, format_who(player_chars, linkdead))
                });
            }
            Command::Where => {
                outputs.write(ConnectionOutput {
                    echo: None,
                    ..ConnectionOutput::new(conn, format_where(char_entity, player_chars, rooms))
                });
            }
            Command::Commands => {
                outputs.write(ConnectionOutput {
                    echo: None,
                    ..ConnectionOutput::new(conn, formatter::format_commands())
                });
            }
            Command::Areas => {
                outputs.write(ConnectionOutput {
                    echo: None,
                    ..ConnectionOutput::new(conn, format_areas(rooms))
                });
            }
            Command::Shutdown { .. } | Command::Goto { .. } | Command::Gecho { .. } => {
                // Admin-gated + masked: a non-admin must not learn the command
                // exists, so respond exactly as for an unknown command — same
                // text, same framing (a direct ConnectionOutput, no prepended
                // newline; the engine's InfoMessage path would add a leading
                // newline and leak the difference). One shared helper keeps every
                // admin-gated command byte-identical for non-admins.
                let is_admin = characters
                    .get(char_entity)
                    .map(|(_, c, _)| c.is_admin())
                    .unwrap_or(false);
                dispatch_admin_gated(cmd, is_admin, conn, &mut client.input_queue, outputs);
            }
            _ => {
                // All other commands go through the queue to enforce cooldown
                client.input_queue.push_back(cmd);
            }
        }
    } else if text.trim().is_empty() {
        // Blank line — write a newline to trigger prompt on flush
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(conn, " ")
        });
    } else {
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(conn, tr!("error.unknown_command"))
        });
    }
}

/// The sorted `who` list of connected characters, linkdead ones marked.
fn format_who(
    player_chars: &Query<(Entity, &GrimName, &InRoom, Option<&Character>)>,
    linkdead: &Query<&Linkdead>,
) -> String {
    let mut entries: Vec<String> = player_chars
        .iter()
        .filter(|(_, _, _, c)| c.is_some())
        .map(|(e, n, _, _)| {
            if linkdead.get(e).is_ok() {
                format!("{} (Linkdead)", n.0)
            } else {
                n.0.clone()
            }
        })
        .collect();
    entries.sort();
    formatter::format_who_list(&entries)
}

/// The `where` list: other characters in the actor's current area, by room.
fn format_where(
    char_entity: Entity,
    player_chars: &Query<(Entity, &GrimName, &InRoom, Option<&Character>)>,
    rooms: &RoomResolver,
) -> String {
    let actor_area = player_chars
        .get(char_entity)
        .ok()
        .and_then(|(_, _, ir, _)| rooms.rooms.get(ir.room).ok().map(|(_, r, _)| r.area));
    let mut entries: Vec<(String, String)> = Vec::new();
    if let Some(area) = actor_area {
        for (e, n, ir, _) in player_chars.iter() {
            if e == char_entity {
                continue;
            }
            if let Ok((_, r, rn)) = rooms.rooms.get(ir.room) {
                if r.area == area {
                    entries.push((n.0.clone(), rn.0.clone()));
                }
            }
        }
        entries.sort_by(|a, b| a.1.cmp(&b.1));
    }
    formatter::format_where_list(&entries)
}

/// The sorted, deduped `areas` list of `(friendly_id, name)`.
fn format_areas(rooms: &RoomResolver) -> String {
    let mut entries: Vec<(String, String)> = rooms
        .areas
        .iter()
        .map(|a| (a.friendly_id.clone(), a.name.clone()))
        .collect();
    entries.sort();
    entries.dedup();
    formatter::format_areas_list(&entries)
}

/// Dispatch an admin-gated command: an admin's is queued for the engine; a
/// non-admin gets the exact unknown-command reply (same text, same direct
/// framing — no leading newline) so the command's existence is not leaked.
/// Shared by every admin-gated command so their masked responses stay
/// byte-identical.
pub(crate) fn dispatch_admin_gated(
    cmd: Command,
    is_admin: bool,
    conn: Entity,
    queue: &mut std::collections::VecDeque<Command>,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    if is_admin {
        queue.push_back(cmd);
    } else {
        outputs.write(ConnectionOutput {
            echo: None,
            ..ConnectionOutput::new(conn, tr!("error.unknown_command"))
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_command_queue(
    time: Res<Time>,
    mut clients: Query<(Entity, &mut Client)>,
    mut engine_commands: MessageWriter<EngineCommand>,
    mut announce_logout: MessageWriter<LogoutAnnounce>,
    mut disconnect: MessageWriter<DisconnectRequest>,
    player_chars: Query<(Entity, &GrimName)>,
    characters: Query<&Character>,
    persistence: Res<grim_persistence::PersistenceConfig>,
    mut commands: Commands,
) {
    for (entity, mut client) in clients.iter_mut() {
        let conn = client.connection;
        if client.state != ClientState::InGame {
            continue;
        }
        client.command_cooldown.tick(time.delta());
        if !client.command_cooldown.is_finished() {
            continue;
        }
        if let Some(cmd) = client.input_queue.pop_front() {
            if matches!(&cmd, Command::Quit) {
                let char_name = client
                    .character
                    .and_then(|c| player_chars.get(c).ok())
                    .map(|(_, n)| n.0.clone())
                    .unwrap_or_else(|| "Someone".into());
                // `quit` is an intentional logout: save the character to disk,
                // then DESPAWN its entity entirely — a logged-out character lives
                // only on disk and is re-loaded on next login. This is NOT
                // linkdead: linkdead is only for an *unexpected* socket drop (see
                // `save_on_disconnect`). The Client is despawned too, so the
                // ensuing `ConnectionClosed` finds no client and no linkdead is set.
                if let Some(char_entity) = client.character {
                    if let Ok(ch) = characters.get(char_entity) {
                        let path = persistence
                            .characters_dir()
                            .join(format!("{}.json", ch.name));
                        let _ = std::fs::create_dir_all(persistence.characters_dir());
                        if let Ok(json) = serde_json::to_string_pretty(ch) {
                            let _ = std::fs::write(path, json);
                        }
                    }
                    commands.entity(char_entity).despawn();
                }
                commands.entity(entity).despawn();
                announce_logout.write(LogoutAnnounce {
                    name: char_name.clone(),
                });
                info!("Character '{}' quit", char_name);
                disconnect.write(DisconnectRequest { connection: conn });
                continue;
            }
            engine_commands.write(EngineCommand {
                client: client.character.unwrap_or(entity),
                command: cmd,
            });
            // Start cooldown for next command
            client.command_cooldown.reset();
        }
    }
}
