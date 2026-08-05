//! Entering the world as a named character: shared by login-by-name and the
//! character-menu selection. Resolves whether to reconnect a linkdead entity,
//! take over a live/offline one, or spawn a fresh entity from disk.

use bevy::prelude::*;
use grim_engine_types::components::{
    Character, Client, ClientState, Description, InRoom, Linkdead, Name as GrimName, OutputHistory,
    Player,
};
use grim_engine_types::events::LinkdeadAnnounce;
use grim_engine_types::GrimId;
use grim_networking::{ConnectionOutput, DisconnectRequest};
use grim_persistence::{load_character_by_name, PersistenceConfig};
use grim_text::tr;

use crate::formatter;
use crate::params::RoomResolver;

/// Enter the world as a named character owned by the just-authed account. Used
/// by both the login-by-name auto-select path and the character-menu selection,
/// so the two share one placement/reconnect/takeover routine.
///
/// - Resident + linkdead → reconnect (replay buffered output).
/// - Resident + online/offline → takeover (disconnect any old session), place.
/// - Not resident → load from disk and spawn a fresh in-world entity.
///
/// Fails closed: the character's `account_id` must equal `account_id`, and an
/// unknown name is refused — both return the client to the login prompt.
#[allow(clippy::too_many_arguments)]
pub(crate) fn enter_world_by_name(
    conn: Entity,
    client: &mut Client,
    account_id: GrimId,
    name: &str,
    commands: &mut Commands,
    characters: &Query<(Entity, &Character, &GrimName)>,
    players: &Query<&Player>,
    linkdead: &Query<&Linkdead>,
    histories: &mut Query<&mut OutputHistory>,
    rooms: &RoomResolver,
    starting: Entity,
    persistence: &PersistenceConfig,
    outputs: &mut MessageWriter<ConnectionOutput>,
    announce_linkdead: &mut MessageWriter<LinkdeadAnnounce>,
    disconnect: &mut MessageWriter<DisconnectRequest>,
) {
    let refuse = |client: &mut Client, outputs: &mut MessageWriter<ConnectionOutput>, msg: &str| {
        client.state = ClientState::LoginPrompt;
        outputs.write(ConnectionOutput {
            echo: Some(true),
            ..ConnectionOutput::new(conn, format!("{msg}\n{}", tr!("login.prompt")))
        });
    };

    // Prefer a resident entity for this name (linkdead beats online).
    let resident = characters
        .iter()
        .filter(|(_, _, n)| n.0.eq_ignore_ascii_case(name))
        .max_by_key(|(e, _, _)| if linkdead.get(*e).is_ok() { 1 } else { 0 })
        .map(|(e, c, _)| (e, c.account_id));

    if let Some((char_entity, char_account)) = resident {
        // Fail closed: the character must belong to the authed account.
        if char_account != account_id {
            refuse(
                client,
                outputs,
                "That character does not belong to this account.",
            );
            return;
        }
        if linkdead.get(char_entity).is_ok() {
            reconnect_linkdead(
                char_entity,
                conn,
                name,
                client,
                commands,
                histories,
                outputs,
                announce_linkdead,
            );
        } else {
            takeover_resident(
                char_entity,
                conn,
                client,
                commands,
                characters,
                players,
                rooms,
                starting,
                outputs,
                disconnect,
            );
        }
        return;
    }

    // Not resident: bring the character in from disk.
    match load_character_by_name(persistence, name) {
        Some(loaded) if loaded.account_id == account_id => {
            spawn_from_disk(loaded, conn, client, commands, rooms, starting, outputs);
        }
        Some(_) => refuse(
            client,
            outputs,
            "That character does not belong to this account.",
        ),
        None => refuse(client, outputs, "That character could not be found."),
    }
}

/// Reconnect a linkdead character on a new connection: clear `Linkdead`, adopt
/// the socket, and discard the pre-disconnect output buffer (no replay).
#[allow(clippy::too_many_arguments)]
fn reconnect_linkdead(
    char_entity: Entity,
    conn: Entity,
    name: &str,
    client: &mut Client,
    commands: &mut Commands,
    histories: &mut Query<&mut OutputHistory>,
    outputs: &mut MessageWriter<ConnectionOutput>,
    announce_linkdead: &mut MessageWriter<LinkdeadAnnounce>,
) {
    commands.entity(char_entity).remove::<Linkdead>();
    commands.entity(char_entity).insert(Player {
        connection: Some(conn),
    });
    client.character = Some(char_entity);
    client.state = ClientState::InGame;
    client.input_queue = std::collections::VecDeque::new();
    client.command_cooldown = Timer::from_seconds(0.5, TimerMode::Once);
    outputs.write(ConnectionOutput {
        echo: None,
        ..ConnectionOutput::new(conn, "Reconnecting...\n")
    });
    // No playback for now: discard buffered output from before the disconnect
    // and don't auto-look. The player just gets the reconnection notice; they
    // can `look` themselves.
    if let Ok(mut history) = histories.get_mut(char_entity) {
        history.drain();
    }
    announce_linkdead.write(LinkdeadAnnounce {
        name: name.to_string(),
        reconnecting: true,
    });
    info!("Character '{name}' reconnected");
    // Start fresh output capture on the new connection.
    commands.entity(conn).insert(OutputHistory::with_max(100));
}

/// Take over an online/offline resident entity: kick any existing session,
/// re-place the character, and advance the new session to the MOTD.
#[allow(clippy::too_many_arguments)]
fn takeover_resident(
    char_entity: Entity,
    conn: Entity,
    client: &mut Client,
    commands: &mut Commands,
    characters: &Query<(Entity, &Character, &GrimName)>,
    players: &Query<&Player>,
    rooms: &RoomResolver,
    starting: Entity,
    outputs: &mut MessageWriter<ConnectionOutput>,
    disconnect: &mut MessageWriter<DisconnectRequest>,
) {
    // Online/offline resident → takeover: kick any existing session.
    if let Ok(player) = players.get(char_entity) {
        if let Some(old_conn) = player.connection {
            outputs.write(ConnectionOutput::new(
                old_conn,
                "Someone else has logged into this character.\n",
            ));
            disconnect.write(DisconnectRequest {
                connection: old_conn,
            });
        }
    }
    let last = characters
        .get(char_entity)
        .ok()
        .and_then(|(_, c, _)| c.last_room.clone());
    commands.entity(char_entity).insert((
        Player {
            connection: Some(conn),
        },
        InRoom {
            room: rooms.placement(last.as_ref(), starting),
        },
    ));
    client.character = Some(char_entity);
    client.state = ClientState::MotdPrompt;
    outputs.write(ConnectionOutput {
        echo: Some(true),
        ..ConnectionOutput::new(conn, formatter::format_motd())
    });
}

/// Spawn a fresh in-world entity for a character loaded from disk and advance
/// the session to the MOTD.
fn spawn_from_disk(
    loaded: Character,
    conn: Entity,
    client: &mut Client,
    commands: &mut Commands,
    rooms: &RoomResolver,
    starting: Entity,
    outputs: &mut MessageWriter<ConnectionOutput>,
) {
    let last = loaded.last_room.clone();
    let canonical = loaded.name.clone();
    let char_entity = commands
        .spawn((
            loaded,
            GrimName(canonical),
            Description("A new adventurer.".into()),
            Player {
                connection: Some(conn),
            },
            InRoom {
                room: rooms.placement(last.as_ref(), starting),
            },
        ))
        .id();
    client.character = Some(char_entity);
    client.state = ClientState::MotdPrompt;
    outputs.write(ConnectionOutput {
        echo: Some(true),
        ..ConnectionOutput::new(conn, formatter::format_motd())
    });
}
