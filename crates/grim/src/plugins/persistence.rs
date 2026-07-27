use std::fs;
use bevy::prelude::*;
use bevy::log::info;
use crate::components::{Account, Area, Character, Client, Connection, Description, InRoom, Linkdead, Name, Room, RoomLocation};
use crate::events::ConnectionClosed;

/// Loads accounts/characters on startup, saves on disconnect.
pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ConnectionClosed>()
            .add_systems(Startup, load_persisted_data)
            .add_systems(Update, save_on_disconnect);
    }
}

/// Spawn every account and character found on disk. Missing directories are
/// treated as empty (no-op) rather than errors.
fn load_persisted_data(mut commands: Commands) {
    let _ = fs::create_dir_all("data/accounts");
    let _ = fs::create_dir_all("data/characters");

    if let Ok(entries) = fs::read_dir("data/accounts") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(account) = serde_json::from_str::<Account>(&data) {
                    commands.spawn(account);
                }
            }
        }
    }

    if let Ok(entries) = fs::read_dir("data/characters") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(character) = serde_json::from_str::<Character>(&data) {
                    let name = character.name.clone();
                    commands.spawn((
                        character,
                        Name(name.into()),
                        Description("A new adventurer.".into()),
                    ));
                }
            }
        }
    }
}

/// On connection close: persist the bound account/character (refreshing the
/// character's `last_room` from its current `InRoom`), then mark the character
/// as Linkdead and despawn the client/connection entities.
fn save_on_disconnect(
    mut commands: Commands,
    mut closed: MessageReader<ConnectionClosed>,
    clients: Query<(Entity, &Client)>,
    connections: Query<&Connection>,
    accounts: Query<&Account>,
    mut characters: Query<&mut Character>,
    inroom: Query<&InRoom>,
    rooms: Query<(&Room, &Area)>,
) {
    for ev in closed.read() {
        let conn = ev.connection;

        if let Ok(c) = connections.get(conn) {
            info!("Connection closed from {}", c.addr);
        }

        let mut client_entity = None;
        let mut account_entity = None;
        let mut character_entity = None;
        for (e, client) in clients.iter() {
            if client.connection == conn {
                client_entity = Some(e);
                account_entity = client.account;
                character_entity = client.character;
                break;
            }
        }
        let Some(client_e) = client_entity else {
            // No client for this connection; still despawn the connection.
            commands.entity(conn).despawn();
            continue;
        };

        if let Some(acct_e) = account_entity {
            if let Ok(account) = accounts.get(acct_e) {
                let path = format!("data/accounts/{}.json", account.id);
                if let Ok(json) = serde_json::to_string_pretty(account) {
                    let _ = fs::write(path, json);
                }
            }
        }

        if let Some(char_e) = character_entity {
            if let Ok(mut character) = characters.get_mut(char_e) {
                if let Ok(ir) = inroom.get(char_e) {
                    if let Ok((room, area)) = rooms.get(ir.room) {
                        character.last_room = Some(RoomLocation {
                            area: area.friendly_id.clone(),
                            room: room.friendly_id.clone(),
                        });
                    }
                }
                let path = format!("data/characters/{}.json", character.id);
                if let Ok(json) = serde_json::to_string_pretty(&*character) {
                    let _ = fs::write(path, json);
                }
            }
            // Add Linkdead instead of despawning the character entity
            commands.entity(char_e).insert(Linkdead);
            if let Ok(ch) = characters.get(char_e) {
                info!("Character '{}' went linkdead", ch.name);
            }
        }

        commands.entity(client_e).despawn();
        commands.entity(conn).despawn();
    }
}
