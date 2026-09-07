//! Copyover resume: re-adopt a character whose socket was carried across a
//! hot restart. The transport already rebuilt the connection, so this skips the
//! login flow and drops the character straight back into the world at its
//! persisted `last_room` (falling back to the starting room).

use bevy::prelude::*;
use chrono::Utc;
use grim_actor::{Character, InRoom, Linkdead, OutputHistory, Player};
use grim_core::components::{Account, Client, ClientState, Description, Name as GrimName};
use grim_core::events::LookRoom;
use grim_networking::{Connection, ConnectionOutput, ConnectionResumed, DisconnectRequest};
use grim_persistence::{load_character_by_name, BanList, PersistenceConfig};
use grim_text::tr;
use grim_world::StartingRoom;

use crate::params::RoomResolver;
use crate::scene_stack::push_ingame_scene;
use crate::session::ConnectedAt;

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_connection_resumed(
    mut resumed: MessageReader<ConnectionResumed>,
    mut commands: Commands,
    characters: Query<(Entity, &Character, &GrimName)>,
    accounts: Query<(Entity, &Account)>,
    players: Query<&Player>,
    rooms: RoomResolver,
    starting: Res<StartingRoom>,
    persistence: Res<PersistenceConfig>,
    bans: Res<BanList>,
    connections: Query<&Connection>,
    mut outputs: MessageWriter<ConnectionOutput>,
    mut look_room: MessageWriter<LookRoom>,
    mut disconnect: MessageWriter<DisconnectRequest>,
) {
    for ev in resumed.read() {
        let conn = ev.connection;
        // Banned identities never reach placement: refuse before
        // `resolve_resumed` spawns or attaches anything.
        if refuse_banned(
            ev,
            &bans,
            &connections,
            &characters,
            &accounts,
            &persistence,
            &mut outputs,
            &mut disconnect,
        ) {
            continue;
        }
        // Resolve (and place) the character; a failed resolve fails closed by
        // dropping the socket inside `resolve_resumed`.
        let Some((account_entity, char_entity, room)) = resolve_resumed(
            ev,
            &mut commands,
            &characters,
            &accounts,
            &players,
            &rooms,
            starting.0,
            &persistence,
            &mut outputs,
            &mut disconnect,
        ) else {
            continue;
        };
        finalize_resume(
            conn,
            account_entity,
            char_entity,
            room,
            &mut commands,
            &mut outputs,
            &mut look_room,
        );
    }
}

/// Refuse a resumed socket whose IP, character, or owning account is banned.
/// Runs before [`resolve_resumed`] so a refused socket spawns and attaches
/// nothing; the account resolves off the resident entity or disk without side
/// effects. Returns true when the socket was refused.
#[allow(clippy::too_many_arguments)]
fn refuse_banned(
    ev: &ConnectionResumed,
    bans: &BanList,
    connections: &Query<&Connection>,
    characters: &Query<(Entity, &Character, &GrimName)>,
    accounts: &Query<(Entity, &Account)>,
    persistence: &PersistenceConfig,
    outputs: &mut MessageWriter<ConnectionOutput>,
    disconnect: &mut MessageWriter<DisconnectRequest>,
) -> bool {
    let conn = ev.connection;
    let mut refuse = |text: String| {
        outputs.write(ConnectionOutput::new(conn, text));
        disconnect.write(DisconnectRequest { connection: conn });
    };
    if connections
        .get(conn)
        .is_ok_and(|c| bans.is_ip_banned(&c.addr.ip()))
    {
        refuse(tr!("ban.banned.ip"));
        return true;
    }
    if bans.is_character_banned(&ev.character) {
        refuse(tr!("ban.banned.character"));
        return true;
    }
    let account_id = characters
        .iter()
        .find(|(_, _, n)| n.0 == ev.character)
        .map(|(_, c, _)| c.account_id)
        .or_else(|| load_character_by_name(persistence, &ev.character).map(|c| c.account_id));
    if account_id
        .and_then(|id| accounts.iter().find(|(_, a)| a.id == id))
        .is_some_and(|(_, a)| bans.is_account_banned(a))
    {
        refuse(tr!("ban.banned.account"));
        return true;
    }
    false
}

/// Resolve the resumed character to `(account, character, room)`, placing it in
/// the world. In the new lifecycle a logged-out character lives only on disk, so
/// a resumed character may not be resident yet — spawn it from disk if needed.
///
/// Fails closed: if anything about the character is missing or in an unexpected
/// state, drop the socket (emit the reconnect notice + disconnect) and return
/// `None` rather than log in a half-built or corrupt session.
#[allow(clippy::too_many_arguments)]
fn resolve_resumed(
    ev: &ConnectionResumed,
    commands: &mut Commands,
    characters: &Query<(Entity, &Character, &GrimName)>,
    accounts: &Query<(Entity, &Account)>,
    players: &Query<&Player>,
    rooms: &RoomResolver,
    starting_room: Entity,
    persistence: &PersistenceConfig,
    outputs: &mut MessageWriter<ConnectionOutput>,
    disconnect: &mut MessageWriter<DisconnectRequest>,
) -> Option<(Entity, Entity, Entity)> {
    let conn = ev.connection;
    let abort = |outputs: &mut MessageWriter<ConnectionOutput>,
                 disconnect: &mut MessageWriter<DisconnectRequest>,
                 reason: &str| {
        warn!("copyover resume aborted for '{}': {reason}", ev.character);
        outputs.write(ConnectionOutput::new(
            conn,
            "Your session could not be restored. Please reconnect.\n",
        ));
        disconnect.write(DisconnectRequest { connection: conn });
    };

    if let Some((resident, character, _)) = characters.iter().find(|(_, _, n)| n.0 == ev.character)
    {
        // A resumed character must not already be live (would mean a duplicate
        // handoff or a name collision). Online ⇔ has a `Player`; refuse if so.
        if players.get(resident).is_ok() {
            abort(outputs, disconnect, "character already connected");
            return None;
        }
        // A character with no matching account is corrupt; refuse.
        let Some((acct, _)) = accounts.iter().find(|(_, a)| a.id == character.account_id) else {
            abort(outputs, disconnect, "account not found");
            return None;
        };
        let r = rooms.placement(character.last_room.as_ref(), starting_room);
        // The resident may be linkdead (has `Character`, no `Player`); bringing
        // it back online must both attach the `Player` and clear `Linkdead` in
        // the same deferred command set, or it would be online AND linkdead.
        commands
            .entity(resident)
            .insert((
                Player { connection: conn },
                ConnectedAt(Utc::now()),
                InRoom { room: r },
            ))
            .remove::<Linkdead>();
        Some((acct, resident, r))
    } else {
        // Not resident: load from disk and spawn a fresh entity.
        let Some(loaded) = load_character_by_name(persistence, &ev.character) else {
            abort(outputs, disconnect, "character not found");
            return None;
        };
        let Some((acct, _)) = accounts.iter().find(|(_, a)| a.id == loaded.account_id) else {
            abort(outputs, disconnect, "account not found");
            return None;
        };
        let last = loaded.last_room.clone();
        let r = rooms.placement(last.as_ref(), starting_room);
        // Copyover rebuilds the world from scratch: re-spawn the pack snapshot
        // alongside the character, same as a fresh login.
        let inventory = loaded.inventory.clone();
        let (name, actor, character) = loaded.into_components();
        let char_entity = commands
            .spawn((
                name,
                actor,
                character,
                Description(vec![tr!("character.default_description")]),
                Player { connection: conn },
                ConnectedAt(Utc::now()),
                InRoom { room: r },
            ))
            .id();
        grim_object::persist::restore(commands, char_entity, &inventory);
        Some((acct, char_entity, r))
    }
}

/// Spawn the resumed `Client`, begin output capture, greet, and show the room.
fn finalize_resume(
    conn: Entity,
    account_entity: Entity,
    char_entity: Entity,
    room: Entity,
    commands: &mut Commands,
    outputs: &mut MessageWriter<ConnectionOutput>,
    look_room: &mut MessageWriter<LookRoom>,
) {
    let mut client = Client::new(conn);
    client.state = ClientState::InGame;
    client.account = Some(account_entity);
    client.character = Some(char_entity);
    let session = commands.spawn(client).id();
    // Copyover resume skips login: push the in-game scene directly.
    push_ingame_scene(commands, session);
    // Capture output on the new connection, greet, and show the room.
    commands.entity(conn).insert(OutputHistory::with_max(100));
    outputs.write(ConnectionOutput {
        echo: Some(true),
        ..ConnectionOutput::new(
            conn,
            "{Y[SERVER]{x The world was reloaded. You are back where you left off.\n",
        )
    });
    look_room.write(LookRoom {
        target: char_entity,
        room,
    });
}
