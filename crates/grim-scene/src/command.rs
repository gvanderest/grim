//! In-game command handling: parse a line for a session in `InGame`, answer the
//! session-local commands (who/where/commands/areas), gate admin-only commands,
//! and drain the per-client queue into engine commands under a cooldown.

use std::cmp::Ordering;

use bevy::prelude::*;
use chrono::{DateTime, Utc};
use grim_actor::{Actor, Character, Linkdead, StoredCharacter};
use grim_engine_types::components::{Client, ClientState, Gender, Name as GrimName};
use grim_engine_types::events::{Command, EngineCommand, LogoutAnnounce};
use grim_networking::{ConnectionOutput, DisconnectRequest};
use grim_text::tr;

use crate::formatter::{self, WhoRow};
use crate::params::{PlayerChars, RoomResolver, SessionRes};
use crate::parser;

/// InGame: parse the line (honouring `!` repeat), answer session-local commands
/// directly, admin-gate shutdown/goto, and queue everything else for cooldown.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_ingame(
    client: &mut Client,
    conn: Entity,
    text: &str,
    characters: &Query<(Entity, &Character, &Actor, &GrimName)>,
    player_chars: &PlayerChars,
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
                    ..ConnectionOutput::new(conn, format_who(player_chars, linkdead, res))
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
                    .map(|(_, c, _, _)| c.is_admin())
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

/// The WHO ordering keys for one online character.
struct WhoKey {
    is_admin: bool,
    level: u32,
    connected_at: DateTime<Utc>,
    /// Lower-cased name for a case-insensitive tiebreak.
    sort_name: String,
}

/// One online character's WHO data: the ordering [`WhoKey`] plus the
/// fully-computed [`WhoRow`] to render.
struct WhoData<'a> {
    key: WhoKey,
    row: WhoRow<'a>,
}

/// WHO ordering: admins first, alphabetical by name; then everyone else by
/// level DESC, connect-time ASC (oldest connection first), name ASC.
fn who_order(a: &WhoKey, b: &WhoKey) -> Ordering {
    match (a.is_admin, b.is_admin) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.sort_name.cmp(&b.sort_name),
        (false, false) => b
            .level
            .cmp(&a.level)
            .then_with(|| a.connected_at.cmp(&b.connected_at))
            .then_with(|| a.sort_name.cmp(&b.sort_name)),
    }
}

/// Map a [`Gender`] to its single-character WHO code.
fn gender_char(gender: Gender) -> &'static str {
    match gender {
        Gender::Male => "M",
        Gender::Female => "F",
        Gender::Neutral => "N",
    }
}

/// The MUD-style `who` list. Each online character renders as
/// `LLL G RRRRR CCC GGGGG Name Title` (admins show `IMM` for level; restrings
/// override columns — see [`WhoRow`]). Sort: admins first, alphabetical; then
/// everyone else by level DESC, connect-time ASC, name ASC. Linkdead characters
/// still appear, marked.
fn format_who(player_chars: &PlayerChars, linkdead: &Query<&Linkdead>, res: &SessionRes) -> String {
    let mut data: Vec<WhoData> = player_chars
        .iter()
        .filter_map(|(e, n, _, actor, character, connected)| {
            let ch = character?;
            // Race/level/gender live on the shared `Actor` base now.
            let actor = actor?;
            let is_admin = ch.is_admin();
            let race_abbrev = res
                .races
                .get(&actor.race)
                .map(|r| r.abbrev.clone())
                .unwrap_or_default();
            let class_abbrev = res
                .classes
                .get(&ch.class)
                .map(|c| c.abbrev.clone())
                .unwrap_or_default();
            let level_text = if is_admin {
                "IMM".to_string()
            } else {
                actor.level.to_string()
            };
            Some(WhoData {
                key: WhoKey {
                    is_admin,
                    level: actor.level,
                    // Fall back to creation time if (impossibly) unstamped, so
                    // the tiebreak stays deterministic rather than panicking.
                    connected_at: connected.map_or(ch.created_at, |c| c.0),
                    sort_name: n.0.to_lowercase(),
                },
                row: WhoRow {
                    level: level_text,
                    gender: gender_char(actor.gender).to_string(),
                    race: race_abbrev,
                    class: class_abbrev,
                    guild: String::new(),
                    name: n.0.clone(),
                    title: ch.title.clone(),
                    restrings: &ch.restrings,
                    linkdead: linkdead.get(e).is_ok(),
                },
            })
        })
        .collect();

    data.sort_by(|a, b| who_order(&a.key, &b.key));

    let rows: Vec<WhoRow> = data.into_iter().map(|d| d.row).collect();
    formatter::format_who_list(&rows)
}

/// The `where` list: other characters in the actor's current area, by room.
fn format_where(char_entity: Entity, player_chars: &PlayerChars, rooms: &RoomResolver) -> String {
    let actor_area = player_chars
        .get(char_entity)
        .ok()
        .and_then(|(_, _, ir, _, _, _)| rooms.rooms.get(ir.room).ok().map(|(_, r, _)| r.area));
    let mut entries: Vec<(String, String)> = Vec::new();
    if let Some(area) = actor_area {
        for (e, n, ir, _, _, _) in player_chars.iter() {
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
    characters: Query<(&GrimName, &Actor, &Character)>,
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
                    if let Ok((name, actor, ch)) = characters.get(char_entity) {
                        let stored = StoredCharacter::from_components(name, actor, ch);
                        let path = persistence
                            .characters_dir()
                            .join(format!("{}.json", name.0));
                        let _ = std::fs::create_dir_all(persistence.characters_dir());
                        if let Ok(json) = serde_json::to_string_pretty(&stored) {
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

#[cfg(test)]
mod tests {
    use super::{gender_char, who_order, WhoKey};
    use chrono::{DateTime, TimeZone, Utc};
    use grim_engine_types::components::Gender;
    use std::cmp::Ordering;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn key(is_admin: bool, level: u32, connected: i64, name: &str) -> WhoKey {
        WhoKey {
            is_admin,
            level,
            connected_at: at(connected),
            sort_name: name.to_lowercase(),
        }
    }

    #[test]
    fn gender_char_maps_each_variant() {
        assert_eq!(gender_char(Gender::Male), "M");
        assert_eq!(gender_char(Gender::Female), "F");
        assert_eq!(gender_char(Gender::Neutral), "N");
    }

    #[test]
    fn admins_sort_before_non_admins() {
        // A level-1 admin outranks a level-99 player.
        assert_eq!(
            who_order(&key(true, 1, 100, "Zed"), &key(false, 99, 1, "Aaa")),
            Ordering::Less
        );
        assert_eq!(
            who_order(&key(false, 99, 1, "Aaa"), &key(true, 1, 100, "Zed")),
            Ordering::Greater
        );
    }

    #[test]
    fn admins_sort_alphabetically_case_insensitive() {
        assert_eq!(
            who_order(&key(true, 5, 1, "bob"), &key(true, 5, 1, "Alice")),
            Ordering::Greater
        );
    }

    #[test]
    fn non_admins_sort_by_level_desc_then_connect_then_name() {
        // Higher level first.
        assert_eq!(
            who_order(&key(false, 10, 5, "Bob"), &key(false, 5, 1, "Al")),
            Ordering::Less
        );
        // Same level → oldest connection (smaller timestamp) first.
        assert_eq!(
            who_order(&key(false, 10, 1, "Zed"), &key(false, 10, 9, "Al")),
            Ordering::Less
        );
        // Same level + same connect time → name ascending.
        assert_eq!(
            who_order(&key(false, 10, 5, "Al"), &key(false, 10, 5, "Bob")),
            Ordering::Less
        );
    }

    #[test]
    fn full_ordering_admins_then_level_then_connect() {
        let mut keys = [
            key(false, 10, 30, "Carol"),
            key(true, 1, 99, "Zara"),
            key(false, 10, 10, "Bob"),
            key(true, 1, 1, "Alice"),
            key(false, 5, 5, "Dave"),
        ];
        keys.sort_by(who_order);
        let order: Vec<&str> = keys.iter().map(|k| k.sort_name.as_str()).collect();
        // Admins alpha (alice, zara), then level-10 by connect (bob<carol), then
        // the level-5 player.
        assert_eq!(order, vec!["alice", "zara", "bob", "carol", "dave"]);
    }
}
