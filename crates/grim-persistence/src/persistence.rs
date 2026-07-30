use bevy::log::info;
use bevy::prelude::*;
use grim_engine_types::components::{
    Account, Area, Character, Client, Description, InRoom, Linkdead, Name, OutputHistory, Player,
    Room, RoomLocation,
};
use grim_engine_types::events::LinkdeadAnnounce;
use grim_networking::{Connection, ConnectionClosed};
use std::fs;
use std::path::PathBuf;

/// Where accounts and characters are stored. Defaults to `data/`; a test harness
/// (or an author) inserts a custom directory before adding the plugin to isolate
/// state. `accounts/` and `characters/` live under `dir`.
#[derive(Resource, Clone, Debug)]
pub struct PersistenceConfig {
    pub dir: PathBuf,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self { dir: "data".into() }
    }
}

impl PersistenceConfig {
    pub fn accounts_dir(&self) -> PathBuf {
        self.dir.join("accounts")
    }
    pub fn characters_dir(&self) -> PathBuf {
        self.dir.join("characters")
    }
}

/// Loads accounts/characters on startup, saves on disconnect.
pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        // Only inserts the default when the author/harness hasn't set one.
        app.init_resource::<PersistenceConfig>()
            .add_message::<ConnectionClosed>()
            .add_systems(Startup, load_persisted_data)
            .add_systems(Update, save_on_disconnect);
    }
}

/// Spawn every account and character found on disk. Missing directories are
/// treated as empty (no-op) rather than errors.
fn load_persisted_data(mut commands: Commands, config: Res<PersistenceConfig>) {
    let accounts_dir = config.accounts_dir();
    let characters_dir = config.characters_dir();
    let _ = fs::create_dir_all(&accounts_dir);
    let _ = fs::create_dir_all(&characters_dir);

    if let Ok(entries) = fs::read_dir(&accounts_dir) {
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

    if let Ok(entries) = fs::read_dir(&characters_dir) {
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
    players: Query<&Player>,
    config: Res<PersistenceConfig>,
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
                let path = config.accounts_dir().join(format!("{}.json", account.id));
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
                let path = config
                    .characters_dir()
                    .join(format!("{}.json", character.name));
                if let Ok(json) = serde_json::to_string_pretty(&*character) {
                    let _ = fs::write(path, json);
                }
            }
            // Transfer OutputHistory from connection to character before despawn
            if let Ok(history) = histories.get(conn) {
                commands.entity(char_e).insert(history.clone());
            }
            // Only skip linkdead marking if the character was taken over by
            // another session — meaning the Player.connection is different
            // from the connection being closed.
            let has_other_connection = players
                .get(char_e)
                .ok()
                .and_then(|p| p.connection)
                .is_some_and(|c| c != conn);
            if !has_other_connection {
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
        }
        commands.entity(client_e).despawn();
        commands.entity(conn).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::{LazyLock, Mutex};
    use uuid::Uuid;

    /// Serialises filesystem-touching tests: all load/save tests use
    /// `data/` paths, so running them in parallel races.
    static FS_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(PersistencePlugin);
        app.add_message::<LinkdeadAnnounce>();
        app
    }

    #[test]
    fn test_load_persisted_data_with_valid_account() {
        let _guard = FS_LOCK.lock().unwrap();
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
        let loaded: Vec<&Account> = query.iter(app.world()).collect();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].identifier, "testuser");
        assert_eq!(loaded[0].id, account_id);

        let _ = fs::remove_dir_all("data/accounts");
        let _ = fs::remove_dir_all("data/characters");
    }

    // --- load_persisted_data: character loading ---

    #[test]
    fn test_load_persisted_data_with_valid_character() {
        let _guard = FS_LOCK.lock().unwrap();
        let _ = fs::remove_dir_all("data/accounts");
        let _ = fs::remove_dir_all("data/characters");
        let _ = fs::create_dir_all("data/accounts");
        let _ = fs::create_dir_all("data/characters");

        let character = Character {
            id: Uuid::new_v4(),
            name: "TestHero".into(),
            account_id: Uuid::new_v4(),
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
        };
        let char_path = format!("data/characters/{}.json", character.name);
        fs::write(&char_path, serde_json::to_string(&character).unwrap()).unwrap();

        let mut app = test_app();
        app.update();

        let mut query = app.world_mut().query::<(&Character, &Name, &Description)>();
        let loaded: Vec<_> = query.iter(app.world()).collect();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].0.name, "TestHero");
        assert_eq!(loaded[0].1 .0, "TestHero");
        assert_eq!(loaded[0].2 .0, "A new adventurer.");

        // No account was loaded
        let mut acct_query = app.world_mut().query::<&Account>();
        assert_eq!(acct_query.iter(app.world()).len(), 0);

        let _ = fs::remove_dir_all("data/accounts");
        let _ = fs::remove_dir_all("data/characters");
    }

    #[test]
    fn test_load_persisted_data_with_both_account_and_character() {
        let _guard = FS_LOCK.lock().unwrap();
        let _ = fs::remove_dir_all("data/accounts");
        let _ = fs::remove_dir_all("data/characters");
        let _ = fs::create_dir_all("data/accounts");
        let _ = fs::create_dir_all("data/characters");

        let account = Account {
            id: Uuid::new_v4(),
            identifier: "dualuser".into(),
            password_hash: "hash".into(),
            characters: vec![],
            created_at: Utc::now(),
        };
        let acct_path = format!("data/accounts/{}.json", account.id);
        fs::write(&acct_path, serde_json::to_string(&account).unwrap()).unwrap();

        let character = Character {
            id: Uuid::new_v4(),
            name: "DualHero".into(),
            account_id: account.id,
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
        };
        let char_path = format!("data/characters/{}.json", character.name);
        fs::write(&char_path, serde_json::to_string(&character).unwrap()).unwrap();

        let mut app = test_app();
        app.update();

        let mut acct_query = app.world_mut().query::<&Account>();
        let loaded_accts: Vec<_> = acct_query.iter(app.world()).collect();
        assert_eq!(loaded_accts.len(), 1);
        assert_eq!(loaded_accts[0].identifier, "dualuser");

        let mut char_query = app.world_mut().query::<(&Character, &Name)>();
        let loaded_chars: Vec<_> = char_query.iter(app.world()).collect();
        assert_eq!(loaded_chars.len(), 1);
        assert_eq!(loaded_chars[0].0.name, "DualHero");

        let _ = fs::remove_dir_all("data/accounts");
        let _ = fs::remove_dir_all("data/characters");
    }

    #[test]
    fn test_load_persisted_data_skips_non_json_files() {
        let _guard = FS_LOCK.lock().unwrap();
        let _ = fs::remove_dir_all("data/accounts");
        let _ = fs::remove_dir_all("data/characters");
        let _ = fs::create_dir_all("data/characters");

        fs::write("data/characters/readme.txt", "not json").unwrap();

        let mut app = test_app();
        app.update();

        let mut query = app.world_mut().query::<&Character>();
        assert_eq!(query.iter(app.world()).len(), 0);

        let _ = fs::remove_dir_all("data/characters");
    }

    #[test]
    fn test_load_persisted_data_skips_invalid_json() {
        let _guard = FS_LOCK.lock().unwrap();
        let _ = fs::remove_dir_all("data/accounts");
        let _ = fs::remove_dir_all("data/characters");
        let _ = fs::create_dir_all("data/characters");

        fs::write("data/characters/bad.json", "{{{ not valid json }}}").unwrap();

        let mut app = test_app();
        app.update();

        let mut query = app.world_mut().query::<&Character>();
        assert_eq!(query.iter(app.world()).len(), 0);

        let _ = fs::remove_dir_all("data/characters");
    }

    // --- save_on_disconnect tests ---

    fn setup_save_dirs() {
        let _ = fs::create_dir_all("data/accounts");
        let _ = fs::create_dir_all("data/characters");
    }

    fn cleanup_save_dirs() {
        let _ = fs::remove_dir_all("data/accounts");
        let _ = fs::remove_dir_all("data/characters");
    }

    fn make_connection(app: &mut App, id: usize) -> Entity {
        let addr: std::net::SocketAddr = ([127, 0, 0, 1], 30000 + id as u16).into();
        app.world_mut()
            .spawn(Connection {
                id,
                addr,
                echo_hidden: false,
            })
            .id()
    }

    #[test]
    fn test_save_on_disconnect_no_matching_client() {
        let _guard = FS_LOCK.lock().unwrap();
        setup_save_dirs();

        let mut app = test_app();
        let conn = make_connection(&mut app, 1);
        app.world_mut()
            .write_message(ConnectionClosed { connection: conn });
        app.update();

        // Connection despawned, no client to save
        assert!(app.world().get_entity(conn).is_err());
        // No files written
        assert!(fs::read_dir("data/accounts")
            .map(|mut e| e.next().is_none())
            .unwrap_or(true));
        assert!(fs::read_dir("data/characters")
            .map(|mut e| e.next().is_none())
            .unwrap_or(true));

        cleanup_save_dirs();
    }

    #[test]
    fn test_save_on_disconnect_account_only() {
        let _guard = FS_LOCK.lock().unwrap();
        setup_save_dirs();

        let mut app = test_app();
        let conn = make_connection(&mut app, 1);

        let account = Account {
            id: Uuid::new_v4(),
            identifier: "acctonly".into(),
            password_hash: "hash".into(),
            characters: vec![],
            created_at: Utc::now(),
        };
        let acct_e = app.world_mut().spawn(account.clone()).id();

        let client = Client {
            account: Some(acct_e),
            character: None, // no character bound
            ..Client::new(conn)
        };
        let client_e = app.world_mut().spawn(client).id();

        app.world_mut()
            .write_message(ConnectionClosed { connection: conn });
        app.update();

        // Account file written
        let acct_path = format!("data/accounts/{}.json", account.id);
        let saved: Account =
            serde_json::from_str(&fs::read_to_string(&acct_path).unwrap()).unwrap();
        assert_eq!(saved.identifier, "acctonly");

        // No character file written (no character bound)
        assert!(fs::read_dir("data/characters")
            .map(|mut e| e.next().is_none())
            .unwrap_or(true));

        // Client + connection despawned
        assert!(app.world().get_entity(client_e).is_err());
        assert!(app.world().get_entity(conn).is_err());
        // Account entity still alive
        assert!(app.world().get_entity(acct_e).is_ok());

        cleanup_save_dirs();
    }

    #[test]
    fn test_save_on_disconnect_character_without_account_entity() {
        let _guard = FS_LOCK.lock().unwrap();
        setup_save_dirs();

        let mut app = test_app();
        let conn = make_connection(&mut app, 1);

        let character = Character {
            id: Uuid::new_v4(),
            name: "NoAccountHero".into(),
            account_id: Uuid::new_v4(), // orphan — no Account entity spawned
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
        };
        let char_e = app.world_mut().spawn(character.clone()).id();

        let client = Client {
            account: None, // no account entity
            character: Some(char_e),
            ..Client::new(conn)
        };
        let client_e = app.world_mut().spawn(client).id();

        app.world_mut()
            .write_message(ConnectionClosed { connection: conn });
        app.update();

        // Character file written (character_entity is Some)
        let char_path = format!("data/characters/{}.json", character.name);
        let saved: Character =
            serde_json::from_str(&fs::read_to_string(&char_path).unwrap()).unwrap();
        assert_eq!(saved.name, "NoAccountHero");

        // Character marked linkdead
        let mut ld = app.world_mut().query::<&Linkdead>();
        assert_eq!(ld.iter(app.world()).len(), 1);

        // Client + connection despawned
        assert!(app.world().get_entity(client_e).is_err());
        assert!(app.world().get_entity(conn).is_err());

        cleanup_save_dirs();
    }

    #[test]
    fn test_save_on_disconnect_full_save_with_linkdead() {
        let _guard = FS_LOCK.lock().unwrap();
        setup_save_dirs();

        let mut app = test_app();
        let conn = make_connection(&mut app, 1);

        let account = Account {
            id: Uuid::new_v4(),
            identifier: "fullsave".into(),
            password_hash: "hash".into(),
            characters: vec![],
            created_at: Utc::now(),
        };
        let acct_e = app.world_mut().spawn(account.clone()).id();

        let character = Character {
            id: Uuid::new_v4(),
            name: "FullSaveHero".into(),
            account_id: account.id,
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
        };
        let char_e = app.world_mut().spawn(character.clone()).id();

        let client = Client {
            account: Some(acct_e),
            character: Some(char_e),
            ..Client::new(conn)
        };
        let client_e = app.world_mut().spawn(client).id();

        app.world_mut()
            .write_message(ConnectionClosed { connection: conn });
        app.update();

        // Both files written
        let acct_path = format!("data/accounts/{}.json", account.id);
        let saved_acct: Account =
            serde_json::from_str(&fs::read_to_string(&acct_path).unwrap()).unwrap();
        assert_eq!(saved_acct.identifier, "fullsave");

        let char_path = format!("data/characters/{}.json", character.name);
        let saved_char: Character =
            serde_json::from_str(&fs::read_to_string(&char_path).unwrap()).unwrap();
        assert_eq!(saved_char.name, "FullSaveHero");

        // Character marked linkdead with Player { connection: None }
        let mut ld = app.world_mut().query::<(&Character, &Linkdead, &Player)>();
        let results: Vec<_> = ld.iter(app.world()).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.name, "FullSaveHero");
        assert!(results[0].2.connection.is_none());

        // LinkdeadAnnounce emitted
        let messages = app.world().resource::<Messages<LinkdeadAnnounce>>();
        let mut cursor = messages.get_cursor();
        let events: Vec<_> = cursor.read(messages).collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "FullSaveHero");
        assert!(!events[0].reconnecting);

        // Client + connection despawned
        assert!(app.world().get_entity(client_e).is_err());
        assert!(app.world().get_entity(conn).is_err());

        cleanup_save_dirs();
    }

    #[test]
    fn test_save_on_disconnect_updates_last_room_from_inroom() {
        let _guard = FS_LOCK.lock().unwrap();
        setup_save_dirs();

        let mut app = test_app();
        let conn = make_connection(&mut app, 1);

        let room_e = app
            .world_mut()
            .spawn((
                Room {
                    id: Uuid::new_v4(),
                    friendly_id: "test_room".into(),
                    name: "Test Room".into(),
                    description: "".into(),
                    area: Entity::PLACEHOLDER,
                },
                Area {
                    id: Uuid::new_v4(),
                    friendly_id: "test_area".into(),
                    name: "Test Area".into(),
                },
            ))
            .id();
        // Patch room.area to point to itself
        app.world_mut().entity_mut(room_e).insert(Room {
            id: Uuid::new_v4(),
            friendly_id: "test_room".into(),
            name: "Test Room".into(),
            description: "".into(),
            area: room_e,
        });

        let account = Account {
            id: Uuid::new_v4(),
            identifier: "roomtest".into(),
            password_hash: "hash".into(),
            characters: vec![],
            created_at: Utc::now(),
        };
        let acct_e = app.world_mut().spawn(account.clone()).id();

        let character = Character {
            id: Uuid::new_v4(),
            name: "RoomHero".into(),
            account_id: account.id,
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(), // starts with no last_room
        };
        let char_e = app
            .world_mut()
            .spawn((character.clone(), InRoom { room: room_e }))
            .id();

        let client = Client {
            account: Some(acct_e),
            character: Some(char_e),
            ..Client::new(conn)
        };
        let _ = app.world_mut().spawn(client).id();

        app.world_mut()
            .write_message(ConnectionClosed { connection: conn });
        app.update();

        // Character file shows last_room populated from InRoom
        let char_path = format!("data/characters/{}.json", character.name);
        let saved: Character =
            serde_json::from_str(&fs::read_to_string(&char_path).unwrap()).unwrap();
        let loc = saved.last_room.expect("expected last_room to be set");
        assert_eq!(loc.area, "test_area");
        assert_eq!(loc.room, "test_room");

        cleanup_save_dirs();
    }

    #[test]
    fn test_save_on_disconnect_transfers_output_history() {
        let _guard = FS_LOCK.lock().unwrap();
        setup_save_dirs();

        let mut app = test_app();
        let conn = make_connection(&mut app, 1);

        // OutputHistory on the connection entity
        let mut history = OutputHistory::with_max(100);
        history.push("line 1");
        history.push("line 2");
        app.world_mut().entity_mut(conn).insert(history.clone());

        let account = Account {
            id: Uuid::new_v4(),
            identifier: "historytest".into(),
            password_hash: "hash".into(),
            characters: vec![],
            created_at: Utc::now(),
        };
        let acct_e = app.world_mut().spawn(account.clone()).id();

        let character = Character {
            id: Uuid::new_v4(),
            name: "HistoryHero".into(),
            account_id: account.id,
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
        };
        let char_e = app.world_mut().spawn(character.clone()).id();

        let client = Client {
            account: Some(acct_e),
            character: Some(char_e),
            ..Client::new(conn)
        };
        let _ = app.world_mut().spawn(client).id();

        app.world_mut()
            .write_message(ConnectionClosed { connection: conn });
        app.update();

        // Character should now have the OutputHistory
        let mut hist_query = app.world_mut().query::<(&Character, &OutputHistory)>();
        let results: Vec<_> = hist_query.iter(app.world()).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.name, "HistoryHero");
        assert_eq!(
            results[0].1.lines.iter().collect::<Vec<_>>(),
            vec!["line 1", "line 2"]
        );

        cleanup_save_dirs();
    }

    #[test]
    fn test_save_on_disconnect_skips_save_on_get_mut_failure() {
        let _guard = FS_LOCK.lock().unwrap();
        setup_save_dirs();

        let mut app = test_app();
        let conn = make_connection(&mut app, 1);

        let account = Account {
            id: Uuid::new_v4(),
            identifier: "errortest".into(),
            password_hash: "hash".into(),
            characters: vec![],
            created_at: Utc::now(),
        };
        let acct_e = app.world_mut().spawn(account.clone()).id();

        // Character entity but NO Character component — characters.get_mut will fail
        let char_e = app.world_mut().spawn(()).id();

        let client = Client {
            account: Some(acct_e),
            character: Some(char_e),
            ..Client::new(conn)
        };
        let client_e = app.world_mut().spawn(client).id();

        app.world_mut()
            .write_message(ConnectionClosed { connection: conn });
        app.update();

        // Account file still written (account save is independent)
        let acct_path = format!("data/accounts/{}.json", account.id);
        let saved: Account =
            serde_json::from_str(&fs::read_to_string(&acct_path).unwrap()).unwrap();
        assert_eq!(saved.identifier, "errortest");

        // No character file written (get_mut failed)
        assert!(fs::read_dir("data/characters")
            .map(|mut e| e.next().is_none())
            .unwrap_or(true));

        // Client + connection still despawned cleanup
        assert!(app.world().get_entity(client_e).is_err());
        assert!(app.world().get_entity(conn).is_err());

        cleanup_save_dirs();
    }

    #[test]
    fn test_save_on_disconnect_skips_linkdead_announce_on_query_failure() {
        let _guard = FS_LOCK.lock().unwrap();
        setup_save_dirs();

        let mut app = test_app();
        let conn = make_connection(&mut app, 1);

        // Character entity but NO Character component — save_on_disconnect
        // still reaches the OutputHistory/Player/Linkdead block (line 123-136)
        // but characters.get(char_e) at line 131 will fail, skipping the
        // LinkdeadAnnounce.
        let char_e = app.world_mut().spawn(()).id();

        let client = Client {
            account: None,
            character: Some(char_e),
            ..Client::new(conn)
        };
        let client_e = app.world_mut().spawn(client).id();

        app.world_mut()
            .write_message(ConnectionClosed { connection: conn });
        app.update();

        // No LinkdeadAnnounce emitted (characters.get failed at line 131)
        let messages = app.world().resource::<Messages<LinkdeadAnnounce>>();
        let mut cursor = messages.get_cursor();
        assert!(cursor.read(messages).next().is_none());

        // Client + connection despawned
        assert!(app.world().get_entity(client_e).is_err());
        assert!(app.world().get_entity(conn).is_err());

        cleanup_save_dirs();
    }

    /// A non-default `PersistenceConfig` must redirect both load and save, and
    /// nothing may leak to the default `data/` root. No `FS_LOCK`: the temp root
    /// is unique, so this test is isolated from the `data/`-based ones.
    #[test]
    fn configured_directory_redirects_load_and_save() {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("grim-pers-cfg-{}-{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("characters")).unwrap();

        // Seed a character file in the configured dir → it must load from there.
        let seeded = Character {
            id: Uuid::new_v4(),
            name: "CfgHero".into(),
            account_id: Uuid::new_v4(),
            created_at: Utc::now(),
            last_room: None,
            roles: Vec::new(),
        };
        fs::write(
            dir.join("characters").join("CfgHero.json"),
            serde_json::to_string(&seeded).unwrap(),
        )
        .unwrap();

        let mut app = App::new();
        app.insert_resource(PersistenceConfig { dir: dir.clone() });
        app.add_plugins(MinimalPlugins)
            .add_plugins(PersistencePlugin);
        app.add_message::<LinkdeadAnnounce>();
        app.update(); // Startup load

        let mut cq = app.world_mut().query::<&Character>();
        assert!(
            cq.iter(app.world()).any(|c| c.name == "CfgHero"),
            "load must read the configured directory"
        );

        // Save: disconnect an account; the file must land in the configured dir.
        let conn = make_connection(&mut app, 1);
        let account = Account {
            id: Uuid::new_v4(),
            identifier: "cfguser".into(),
            password_hash: "hash".into(),
            characters: vec![],
            created_at: Utc::now(),
        };
        let acct_id = account.id;
        let acct_e = app.world_mut().spawn(account).id();
        let client = Client {
            account: Some(acct_e),
            character: None,
            ..Client::new(conn)
        };
        app.world_mut().spawn(client);
        app.world_mut()
            .write_message(ConnectionClosed { connection: conn });
        app.update();

        assert!(
            dir.join("accounts")
                .join(format!("{acct_id}.json"))
                .exists(),
            "save must write to the configured directory"
        );
        assert!(
            !std::path::Path::new(&format!("data/accounts/{acct_id}.json")).exists(),
            "nothing may leak to the default data/ root"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
