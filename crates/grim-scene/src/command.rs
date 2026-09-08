//! In-game command handling: parse a line for a session in `InGame`, answer the
//! session-local commands (who/where/sockets/commands/areas), gate admin-only
//! and drain the per-client queue into engine commands under a cooldown.

use bevy::prelude::*;
use grim_actor::{Actor, Character, Linkdead, StoredCharacter};
use grim_core::components::{Account, Client, ClientState, Description, Name as GrimName};
use grim_core::events::{Command, DescOp, EngineCommand, LogoutAnnounce};
use grim_networking::{Connection, ConnectionOutput, DisconnectRequest};
use grim_persistence::PersistenceConfig;
use grim_text::tr;

use crate::finger;
use crate::formatter;
use crate::params::{PlayerChars, RoomResolver, SessionRes};
use crate::parser;
use crate::sockets::{format_sockets, ClientSnapshot};
use crate::who::{format_areas, format_where, format_who, format_wizlist};

/// `equipment` stays a dummy (no worn-items system yet). Factored out of
/// [`handle_ingame`] to hold that dispatch table under the line budget.
fn answer_equipment(conn: Entity, outputs: &mut MessageWriter<ConnectionOutput>) {
    outputs.write(ConnectionOutput {
        echo: None,
        ..ConnectionOutput::new(conn, tr!("equipment.empty"))
    });
}

/// Answer the `wizlist` from live characters plus the startup admin snapshot.
/// Factored out of [`handle_ingame`] to hold that dispatch table under the
/// line budget (same reason as [`answer_equipment`]).
fn answer_wizlist(
    conn: Entity,
    player_chars: &PlayerChars,
    linkdead: &Query<&Linkdead>,
    res: &SessionRes,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    outputs.write(ConnectionOutput {
        echo: None,
        ..ConnectionOutput::new(
            conn,
            format_wizlist(player_chars, linkdead, res, &res.wizlist),
        )
    });
}

/// Whether the actor behind `char_entity` holds the admin role. A missing
/// actor reads as non-admin (fail closed). Shared by the session-local
/// (`sockets`) and engine-queued (`shutdown`/`goto`/`gecho`/`ban`) gates so
/// every admin check stays one lookup.
fn character_is_admin(
    characters: &Query<(Entity, &Character, &Actor, &GrimName)>,
    char_entity: Entity,
) -> bool {
    characters
        .get(char_entity)
        .map(|(_, c, _, _)| c.is_admin())
        .unwrap_or(false)
}

/// InGame: parse the line (honouring `!` repeat), answer session-local commands
/// directly, admin-gate shutdown/goto, and queue everything else for cooldown.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_ingame(
    client: &mut Client,
    conn: Entity,
    text: &str,
    characters: &Query<(Entity, &Character, &Actor, &GrimName)>,
    player_chars: &PlayerChars,
    descriptions: &Query<&Description>,
    persistence: &PersistenceConfig,
    linkdead: &Query<&Linkdead>,
    rooms: &RoomResolver,
    res: &SessionRes,
    snapshot: &[ClientSnapshot],
    connections: &Query<&Connection>,
    accounts: &Query<&Account>,
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
                    ..ConnectionOutput::new(conn, format_who(player_chars, linkdead, res))
                });
            }
            Command::Wizlist => {
                answer_wizlist(conn, player_chars, linkdead, res, outputs);
            }
            Command::Where => {
                outputs.write(ConnectionOutput {
                    echo: None,
                    ..ConnectionOutput::new(conn, format_where(char_entity, player_chars, rooms))
                });
            }
            Command::Finger { target } => {
                outputs.write(ConnectionOutput {
                    echo: None,
                    ..ConnectionOutput::new(
                        conn,
                        finger::format(target, player_chars, descriptions, persistence),
                    )
                });
            }
            Command::Desc { op: DescOp::Edit } => {
                crate::editor::open_desc_edit(client, conn, char_entity, descriptions, outputs);
            }
            Command::Equipment => answer_equipment(conn, outputs),
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
            Command::Sockets => {
                // Admin-gated + masked like shutdown/goto/gecho, but answered
                // session-locally (no engine round-trip): a non-admin gets the
                // exact unknown-command reply so the verb is not leaked.
                let is_admin = character_is_admin(characters, char_entity);
                if is_admin {
                    outputs.write(ConnectionOutput {
                        echo: None,
                        ..ConnectionOutput::new(
                            conn,
                            format_sockets(snapshot, connections, characters, accounts),
                        )
                    });
                } else {
                    outputs.write(ConnectionOutput {
                        echo: None,
                        ..ConnectionOutput::new(conn, tr!("error.unknown_command"))
                    });
                }
            }
            Command::Shutdown { .. }
            | Command::Goto { .. }
            | Command::Gecho { .. }
            | Command::Ban { .. } => {
                // Admin-gated + masked: a non-admin must not learn the command
                // exists, so respond exactly as for an unknown command — same
                // text, same framing (a direct ConnectionOutput, no prepended
                // newline; the engine's InfoMessage path would add a leading
                // newline and leak the difference). One shared helper keeps every
                // admin-gated command byte-identical for non-admins.
                let is_admin = character_is_admin(characters, char_entity);
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
    characters: Query<(&GrimName, &Actor, &Character)>,
    persistence: Res<grim_persistence::PersistenceConfig>,
    mut commands: Commands,
    pack: grim_object::persist::Carried,
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
                    if let Ok((name, actor, ch)) = characters.get(char_entity) {
                        let mut stored = StoredCharacter::from_components(name, actor, ch);
                        stored.inventory = grim_object::persist::snapshot_pack(&pack, char_entity);
                        let path = persistence
                            .characters_dir()
                            .join(format!("{}.json", name.0));
                        let _ = std::fs::create_dir_all(persistence.characters_dir());
                        if let Ok(json) = serde_json::to_string_pretty(&stored) {
                            let _ = std::fs::write(path, json);
                        }
                    }
                    // The pack went to disk with the character; despawn the live
                    // objects too, or their `CarriedBy` dangles at this entity.
                    // They re-spawn from the snapshot on next login.
                    let held: Vec<Entity> = pack
                        .iter()
                        .filter(|(_, _, _, _, _, held)| held.carrier == char_entity)
                        .map(|(e, _, _, _, _, _)| e)
                        .collect();
                    grim_object::persist::unload(&mut commands, held.into_iter());
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
