use crate::components::{
    Account, Area, Character, Client, Connection, Description, InRoom, Linkdead, Name,
    OutputHistory, Player, Room, RoomLocation,
};
use crate::events::{ConnectionClosed, LinkdeadAnnounce};
use bevy::log::info;
use bevy::prelude::*;
use std::fs;

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
                        Name(name),
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
#[allow(clippy::too_many_arguments)]
fn save_on_disconnect(
    mut commands: Commands,
    mut closed: MessageReader<ConnectionClosed>,
    clients: Query<(Entity, &Client)>,
    connections: Query<&Connection>,
    accounts: Query<&Account>,
    mut characters: Query<&mut Character>,
    inroom: Query<&InRoom>,
    rooms: Query<(&Room, &Area)>,
    histories: Query<&OutputHistory>,
    mut announce_linkdead: MessageWriter<LinkdeadAnnounce>,
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
                let path = format!("data/characters/{}.json", character.name);
                if let Ok(json) = serde_json::to_string_pretty(&*character) {
                    let _ = fs::write(path, json);
                }
            }
            // Transfer OutputHistory from connection to character before despawn
            if let Ok(history) = histories.get(conn) {
                commands.entity(char_e).insert(history.clone());
            }
            // Mark as linkdead: drop connection, keep Player with None
            commands.entity(char_e).insert(Player { connection: None });
            // Marker component for easy querying
            commands.entity(char_e).insert(Linkdead);
            if let Ok(ch) = characters.get(char_e) {
                info!("Character '{}' went linkdead", ch.name);
                announce_linkdead.write(LinkdeadAnnounce {
                    name: ch.name.clone(),
                    reconnecting: false,
                });
            }
        }
        commands.entity(client_e).despawn();
        commands.entity(conn).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PersistencePlugin);
        app.add_message::<LinkdeadAnnounce>();
        app
    }

    #[test]
    fn test_load_persisted_data_with_valid_account() {
        let _ = fs::remove_dir_all("data/accounts");
        let _ = fs::remove_dir_all("data/characters");
        let account_id = Uuid::new_v4();
        let account = Account {
            id: account_id,
            identifier: "testuser".into(),
            password_hash: "hash".into(),
            characters: vec![],
            created_at: Utc::now(),
        };

        let _ = fs::create_dir_all("data/accounts");
        let _ = fs::create_dir_all("data/characters");
        let account_path = format!("data/accounts/{account_id}.json");
        fs::write(&account_path, serde_json::to_string(&account).unwrap()).unwrap();

        let mut app = test_app();
        app.update();

        let mut query = app.world_mut().query::<&Account>();
        let loaded: Vec<&Account> = query.iter(&app.world()).collect();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].identifier, "testuser");
        assert_eq!(loaded[0].id, account_id);

        let _ = fs::remove_dir_all("data/accounts");
        let _ = fs::remove_dir_all("data/characters");
    }
}
