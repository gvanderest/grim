#[macro_use]
extern crate rust_i18n;
rust_i18n::i18n!("locales");
use bevy::ecs::schedule::IntoScheduleConfigs;

use bevy::log::info;
use bevy::prelude::*;
use chrono::Utc;
use grim::components::{
    Account, Area, Character, Client, ClientState, Description, Exits, InRoom, Linkdead,
    Name as GrimName, OutputHistory, Player, Room, StartingRoom,
};
use grim::events::{
    ClientInput, ClientOutput, Command, ConnectionEstablished, DisconnectRequest, EngineCommand,
    InfoMessage, LinkdeadAnnounce, LoginAnnounce, LogoutAnnounce, LookEntity, LookRoom, MoveEvent,
    OocEvent, SayEvent, YellEvent,
};
use grim::validation::{
    hash_password, validate_character_name, validate_identifier, validate_password, verify_password,
};
use std::collections::VecDeque;
use std::time::Duration;
use uuid::Uuid;

mod formatter;
mod parser;
pub struct ClientPlugin;
impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        rust_i18n::set_locale("en");
        app.add_message::<ClientOutput>()
            .add_message::<DisconnectRequest>()
            .add_message::<EngineCommand>()
            .add_message::<LookRoom>()
            .add_message::<LookEntity>()
            .add_message::<SayEvent>()
            .add_message::<YellEvent>()
            .add_message::<OocEvent>()
            .add_message::<MoveEvent>()
            .add_message::<InfoMessage>()
            .add_message::<LoginAnnounce>()
            .add_message::<LogoutAnnounce>()
            .add_message::<LinkdeadAnnounce>()
            .add_systems(
                Update,
                (
                    handle_connection_established,
                    handle_client_input.after(handle_connection_established),
                    process_command_queue,
                    format_output,
                    capture_output,
                ),
            );
    }
}

fn handle_connection_established(
    mut established: MessageReader<ConnectionEstablished>,
    mut commands: Commands,
    mut outputs: MessageWriter<ClientOutput>,
) {
    for ev in established.read() {
        commands.spawn(Client::new(ev.connection));
        let banner = grim::color::ansi(include_str!("../../../assets/login-banner.txt"));
        let text = format!("{}\r\n\r\n{}", banner, t!("login.prompt"));
        outputs.write(ClientOutput {
            echo: None,
            ..ClientOutput::new(ev.connection, text)
        });
    }
}

// ─── Client input dispatch ───────────────────────────────────────────
#[allow(clippy::too_many_arguments)]
fn handle_client_input(
    mut inputs: MessageReader<ClientInput>,
    mut clients: Query<(Entity, &mut Client)>,
    mut accounts: Query<(Entity, &mut Account)>,
    characters: Query<(Entity, &Character, &GrimName)>,
    player_chars: Query<(Entity, &GrimName, &InRoom, Option<&Character>)>,
    rooms: Query<(&Room, &GrimName)>,
    _areas: Query<&Area>,
    starting: Res<StartingRoom>,
    mut commands: Commands,
    mut outputs: MessageWriter<ClientOutput>,
    mut look_room: MessageWriter<LookRoom>,
    _look_entity: MessageWriter<LookEntity>,
    mut announce_login: MessageWriter<LoginAnnounce>,
    mut announce_linkdead: MessageWriter<LinkdeadAnnounce>,
    linkdead: Query<&Linkdead>,
    mut histories: Query<&mut OutputHistory>,
) {
    for ev in inputs.read() {
        let Some((client_entity, mut client)) = clients
            .iter_mut()
            .find(|(_, c)| c.connection == ev.connection)
        else {
            continue;
        };
        let text = ev.text.as_str();

        let conn = client.connection;
        match &mut client.state {
            ClientState::LoginPrompt => {
                if text.trim().is_empty() {
                    outputs.write(ClientOutput {
                        echo: None,
                        ..ClientOutput::new(conn, t!("login.prompt"))
                    });
                    continue;
                }
                // First, try as character name
                let trimmed = text.trim();
                let char_match = characters
                    .iter()
                    .filter(|(_, _, n)| n.0.eq_ignore_ascii_case(trimmed))
                    .max_by_key(|(e, _, _)| if linkdead.get(*e).is_ok() { 1 } else { 0 });
                if let Some((char_entity, character, _)) = char_match {
                    let account_found = accounts.iter().find(|(_, a)| a.id == character.account_id);
                    if let Some((_account_entity, _)) = account_found {
                        client.state = ClientState::PasswordPrompt {
                            identifier: characters
                                .get(char_entity)
                                .map(|(_, c, _)| {
                                    accounts
                                        .iter()
                                        .find(|(_, a)| a.id == c.account_id)
                                        .map(|(_, a)| a.identifier.clone())
                                        .unwrap_or_default()
                                })
                                .unwrap_or_default(),
                            is_new: false,
                            character: Some(char_entity),
                        };
                        outputs.write(ClientOutput {
                            echo: Some(false),
                            ..ClientOutput::new(conn, "Password: ")
                        });
                        continue;
                    }
                }
                // Fall back to email validation
                match validate_identifier(text) {
                    Ok(identifier) => {
                        let exists = accounts.iter().any(|(_, a)| a.identifier == identifier);
                        if exists {
                            client.state = ClientState::PasswordPrompt {
                                identifier,
                                is_new: false,
                                character: None,
                            };
                            outputs.write(ClientOutput {
                                echo: Some(false),
                                ..ClientOutput::new(conn, "Password: ")
                            });
                        } else {
                            client.state = ClientState::ConfirmCreate {
                                identifier: identifier.clone(),
                            };
                            outputs.write(ClientOutput { echo: None, ..ClientOutput::new(conn, "Did not find that email address, do you want to create an account? [Y/n] ") });
                        }
                    }
                    Err(e) => {
                        outputs.write(ClientOutput { echo: None, ..ClientOutput::new(conn, format!(
                                "Invalid identifier: {}\r\nEnter your character name or email address: ",
                                e
                            )) });
                    }
                }
            }

            ClientState::ConfirmCreate { identifier } => {
                let first = text.trim().to_lowercase();
                // Empty or "y" / "yes" → create account (default Yes)
                if first.is_empty() || first == "y" || first == "yes" {
                    let id = identifier.clone();
                    client.state = ClientState::PasswordPrompt {
                        identifier: id,
                        is_new: true,
                        character: None,
                    };
                    outputs.write(ClientOutput {
                        echo: Some(false),
                        ..ClientOutput::new(conn, "Choose a password: ")
                    });
                } else {
                    client.state = ClientState::LoginPrompt;
                    outputs.write(ClientOutput {
                        echo: None,
                        ..ClientOutput::new(conn, t!("login.prompt"))
                    });
                }
            }
            ClientState::PasswordPrompt {
                identifier,
                is_new,
                character,
            } => {
                // Copy before mutating client to avoid borrow conflict
                let auto_select = *character;
                if text.trim().is_empty() {
                    client.state = ClientState::LoginPrompt;
                    outputs.write(ClientOutput {
                        echo: Some(true),
                        ..ClientOutput::new(conn, t!("login.wrong_password"))
                    });
                    continue;
                }
                if *is_new {
                    match validate_password(text.trim()) {
                        Ok(()) => {
                            let account = Account {
                                id: Uuid::new_v4(),
                                identifier: identifier.clone(),
                                password_hash: hash_password(text.trim()),
                                characters: vec![],
                                created_at: Utc::now(),
                            };
                            // Save to disk immediately
                            let path = format!("data/accounts/{}.json", account.id);
                            if let Ok(json) = serde_json::to_string_pretty(&account) {
                                let _ = std::fs::write(path, json);
                            }
                            let account_entity = commands.spawn(account).id();
                            client.account = Some(account_entity);
                            client.state = ClientState::CharacterSelect;
                            // Restore echo before showing menu
                            outputs.write(ClientOutput {
                                echo: Some(true),
                                ..ClientOutput::new(conn, "")
                            });
                            show_character_menu(
                                client_entity,
                                &client,
                                &characters,
                                &accounts,
                                &mut outputs,
                                &linkdead,
                            );
                        }
                        Err(e) => {
                            outputs.write(ClientOutput {
                                echo: None,
                                ..ClientOutput::new(
                                    conn,
                                    format!("Invalid password: {}\r\nChoose a password: ", e),
                                )
                            });
                        }
                    }
                } else {
                    let account_found = accounts.iter().find(|(_, a)| a.identifier == *identifier);
                    match account_found {
                        Some((account_entity, account)) => {
                            if verify_password(text.trim(), &account.password_hash) {
                                client.account = Some(account_entity);
                                if let Some(char_entity) = auto_select {
                                    // Auto-select character: skip character select
                                    info!("PasswordPrompt auto-select: char_entity={:?}, has_linkdead={}",
                                        char_entity, linkdead.get(char_entity).is_ok());
                                    if linkdead.get(char_entity).is_ok() {
                                        // Linkdead reconnect
                                        commands.entity(char_entity).remove::<Linkdead>();
                                        commands.entity(char_entity).insert(Player {
                                            connection: Some(conn),
                                        });
                                        client.character = Some(char_entity);
                                        client.state = ClientState::InGame;
                                        client.input_queue = VecDeque::new();
                                        client.command_cooldown = {
                                            let mut t = Timer::new(
                                                Duration::from_millis(10),
                                                TimerMode::Repeating,
                                            );
                                            t.set_elapsed(Duration::from_millis(10));
                                            t
                                        };
                                        outputs.write(ClientOutput {
                                            echo: None,
                                            ..ClientOutput::new(conn, "Reconnecting...\r\n")
                                        });
                                        // Replay buffered output from before disconnect
                                        if let Ok(mut history) = histories.get_mut(char_entity) {
                                            for line in history.drain() {
                                                outputs.write(ClientOutput {
                                                    echo: None,
                                                    ..ClientOutput::new(conn, line)
                                                });
                                            }
                                        }
                                        if let Ok((_, _, ir, _)) = player_chars.get(char_entity) {
                                            look_room.write(LookRoom {
                                                target: char_entity,
                                                room: ir.room,
                                            });
                                        }
                                        announce_linkdead.write(LinkdeadAnnounce {
                                            name: characters
                                                .get(char_entity)
                                                .map(|(_, _, n)| n.0.clone())
                                                .unwrap_or_default(),
                                            reconnecting: true,
                                        });
                                        info!("Character reconnected via name login");
                                        // Start fresh output capture on the new connection
                                        commands.entity(conn).insert(OutputHistory::with_max(100));
                                    } else {
                                        commands.entity(char_entity).insert((
                                            Player {
                                                connection: Some(conn),
                                            },
                                            InRoom { room: starting.0 },
                                        ));
                                        client.character = Some(char_entity);
                                        client.state = ClientState::MotdPrompt;
                                        outputs.write(ClientOutput {
                                            echo: Some(true),
                                            ..ClientOutput::new(conn, formatter::format_motd())
                                        });
                                    }
                                } else {
                                    client.state = ClientState::CharacterSelect;
                                    show_character_menu(
                                        client_entity,
                                        &client,
                                        &characters,
                                        &accounts,
                                        &mut outputs,
                                        &linkdead,
                                    );
                                }
                            } else {
                                outputs.write(ClientOutput {
                                    echo: None,
                                    ..ClientOutput::new(conn, "Invalid password.\r\nPassword: ")
                                });
                            }
                        }
                        None => {
                            client.state = ClientState::LoginPrompt;
                            outputs.write(ClientOutput {
                                echo: Some(true),
                                ..ClientOutput::new(
                                    conn,
                                    "Account not found.\r\nEnter your email address: ",
                                )
                            });
                        }
                    }
                }
            }
            ClientState::CharacterSelect => {
                let text = text.trim();
                let lower = text.to_lowercase();
                if lower == "create" || lower == "c" {
                    client.state = ClientState::CreateCharacter;
                    outputs.write(ClientOutput {
                        echo: None,
                        ..ClientOutput::new(conn, "Enter a name for your new character: ")
                    });
                    continue;
                }

                let Some(account_entity) = client.account else {
                    continue;
                };
                let Ok((_, account)) = accounts.get(account_entity) else {
                    continue;
                };
                let mut char_list: Vec<(Entity, Uuid, String)> = characters
                    .iter()
                    .filter(|(_, c, _)| account.characters.contains(&c.id))
                    .map(|(e, c, n)| (e, c.id, n.0.clone()))
                    .collect();
                // Sort so linkdead characters come first (for same name)
                char_list.sort_by_key(|(e, _, name)| {
                    (name.clone(), if linkdead.get(*e).is_ok() { 0 } else { 1 })
                });
                // Try number selection
                let selected = if let Ok(idx) = lower.parse::<usize>() {
                    if idx >= 1 && idx <= char_list.len() {
                        Some(char_list[idx - 1].0)
                    } else {
                        None
                    }
                // Try name selection (case-insensitive)
                } else {
                    char_list
                        .iter()
                        .find(|(_, _, n)| n.to_lowercase() == lower)
                        .map(|&(e, _, _)| e)
                };

                let Some(char_entity) = selected else {
                    show_character_menu(
                        client_entity,
                        &client,
                        &characters,
                        &accounts,
                        &mut outputs,
                        &linkdead,
                    );
                    continue;
                };

                let char_name = characters
                    .get(char_entity)
                    .map(|(_, _, n)| n.0.clone())
                    .ok();
                if linkdead.get(char_entity).is_ok() {
                    // Reconnecting linkdead
                    commands.entity(char_entity).remove::<Linkdead>();
                    commands.entity(char_entity).insert(Player {
                        connection: Some(conn),
                    });
                    client.character = Some(char_entity);
                    client.state = ClientState::InGame;
                    client.input_queue = VecDeque::new();
                    client.command_cooldown = {
                        let mut t = Timer::new(Duration::from_millis(10), TimerMode::Repeating);
                        t.set_elapsed(Duration::from_millis(10));
                        t
                    };
                    outputs.write(ClientOutput {
                        echo: None,
                        ..ClientOutput::new(conn, "Reconnecting...\r\n")
                    });
                    // Replay buffered output from before disconnect
                    if let Ok(mut history) = histories.get_mut(char_entity) {
                        for line in history.drain() {
                            outputs.write(ClientOutput {
                                echo: None,
                                ..ClientOutput::new(conn, line)
                            });
                        }
                    }
                    if let Ok((_, _, ir, _)) = player_chars.get(char_entity) {
                        look_room.write(LookRoom {
                            target: char_entity,
                            room: ir.room,
                        });
                    }
                    announce_linkdead.write(LinkdeadAnnounce {
                        name: char_name.clone().unwrap_or_default(),
                        reconnecting: true,
                    });
                    info!(
                        "Character '{}' reconnected",
                        char_name.as_deref().unwrap_or("?")
                    );
                    // Start fresh output capture on the new connection
                    commands.entity(conn).insert(OutputHistory::with_max(100));
                } else {
                    commands.entity(char_entity).insert((
                        Player {
                            connection: Some(conn),
                        },
                        InRoom { room: starting.0 },
                    ));
                    client.character = Some(char_entity);
                    client.state = ClientState::MotdPrompt;
                    outputs.write(ClientOutput {
                        echo: None,
                        ..ClientOutput::new(conn, formatter::format_motd())
                    });
                }
            }

            ClientState::CreateCharacter => {
                match validate_character_name(text.trim()) {
                    Ok(name) => {
                        let Some(account_entity) = client.account else {
                            continue;
                        };
                        let Ok((_, mut account)) = accounts.get_mut(account_entity) else {
                            continue;
                        };
                        let char_id = Uuid::new_v4();
                        let character = Character {
                            id: char_id,
                            name: name.clone(),
                            account_id: account.id,
                            created_at: Utc::now(),
                            last_room: None,
                        };
                        // Save character to disk immediately
                        let path = format!("data/characters/{}.json", name);
                        if let Ok(json) = serde_json::to_string_pretty(&character) {
                            let _ = std::fs::write(path, json);
                        }
                        let char_entity = commands
                            .spawn((
                                character,
                                GrimName(name.clone()),
                                Description("A new adventurer.".into()),
                                Player {
                                    connection: Some(conn),
                                },
                                InRoom { room: starting.0 },
                            ))
                            .id();
                        account.characters.push(char_id);
                        // Update account JSON with new character reference
                        let acct_path = format!("data/accounts/{}.json", account.id);
                        if let Ok(json) = serde_json::to_string_pretty(&*account) {
                            let _ = std::fs::write(acct_path, json);
                        }
                        client.character = Some(char_entity);
                        client.state = ClientState::MotdPrompt;
                        outputs.write(ClientOutput {
                            echo: None,
                            ..ClientOutput::new(conn, formatter::format_motd())
                        });
                    }
                    Err(e) => {
                        outputs.write(ClientOutput {
                            echo: None,
                            ..ClientOutput::new(
                                conn,
                                format!("Invalid name: {}\r\nEnter a name for your character: ", e),
                            )
                        });
                    }
                }
            }

            ClientState::MotdPrompt => {
                let Some(char_entity) = client.character else {
                    continue;
                };
                let char_name = characters
                    .get(char_entity)
                    .map(|(_, _, n)| n.0.clone())
                    .unwrap_or_else(|_| "Someone".into());
                info!("Character '{}' entered the world", char_name);
                client.state = ClientState::InGame;
                client.input_queue = VecDeque::new();
                client.command_cooldown = {
                    let mut t = Timer::new(Duration::from_millis(10), TimerMode::Repeating);
                    t.set_elapsed(Duration::from_millis(10));
                    t
                };
                // Start output capture now that the character is in the world
                commands.entity(conn).insert(OutputHistory::with_max(100));
                announce_login.write(LoginAnnounce { name: char_name });
                let Some(char_entity) = client.character else {
                    continue;
                };
                if let Ok((_, _, ir, _)) = player_chars.get(char_entity) {
                    look_room.write(LookRoom {
                        target: char_entity,
                        room: ir.room,
                    });
                }
            }

            ClientState::InGame => {
                let Some(char_entity) = client.character else {
                    continue;
                };
                if let Some(cmd) = parser::parse_command(text) {
                    match &cmd {
                        Command::Who => {
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
                            outputs.write(ClientOutput {
                                echo: None,
                                ..ClientOutput::new(conn, formatter::format_who_list(&entries))
                            });
                        }
                        Command::Where => {
                            let actor_area =
                                player_chars
                                    .get(char_entity)
                                    .ok()
                                    .and_then(|(_, _, ir, _)| {
                                        rooms.get(ir.room).ok().map(|(r, _)| r.area)
                                    });
                            let mut entries: Vec<(String, String)> = Vec::new();
                            if let Some(area) = actor_area {
                                for (e, n, ir, _) in player_chars.iter() {
                                    if e == char_entity {
                                        continue;
                                    }
                                    if let Ok((r, rn)) = rooms.get(ir.room) {
                                        if r.area == area {
                                            entries.push((n.0.clone(), rn.0.clone()));
                                        }
                                    }
                                }
                                entries.sort_by(|a, b| a.1.cmp(&b.1));
                            }
                            outputs.write(ClientOutput {
                                echo: None,
                                ..ClientOutput::new(conn, formatter::format_where_list(&entries))
                            });
                        }
                        Command::Commands => {
                            outputs.write(ClientOutput {
                                echo: None,
                                ..ClientOutput::new(conn, formatter::format_commands())
                            });
                        }
                        _ => {
                            client.input_queue.push_back(cmd);
                        }
                    }
                } else if text.trim().is_empty() {
                    // Blank line — write a newline to trigger prompt on flush
                    outputs.write(ClientOutput {
                        echo: None,
                        ..ClientOutput::new(conn, " ")
                    });
                } else {
                    outputs.write(ClientOutput {
                        echo: None,
                        ..ClientOutput::new(
                            conn,
                            "Unknown command. Type 'commands' for a list.\r\n",
                        )
                    });
                }
            }
        }
    }
}

/// Build and send the character selection menu.
fn show_character_menu(
    _client_entity: Entity,
    client: &Client,
    characters: &Query<(Entity, &Character, &GrimName)>,
    accounts: &Query<(Entity, &mut Account)>,
    outputs: &mut MessageWriter<ClientOutput>,
    linkdead: &Query<&Linkdead>,
) {
    let conn = client.connection;
    let Some(account_entity) = client.account else {
        return;
    };
    // Account entity may not exist yet if just created via commands.spawn
    // (deferred execution). Handle gracefully by showing empty menu.
    let welcome = match accounts.get(account_entity) {
        Ok((_, account)) => format!("Welcome back, {}!\r\n", account.identifier),
        Err(_) => "Welcome!\r\n".into(),
    };
    let mut menu = format!("{}\r\n[ Characters ]\r\n", welcome);
    let mut idx = 1;
    for (char_entity, ch, name) in characters.iter() {
        if let Ok((_, account)) = accounts.get(account_entity) {
            if !account.characters.contains(&ch.id) {
                continue;
            }
        }
        let ld_suffix = if linkdead.get(char_entity).is_ok() {
            " (linkdead)"
        } else {
            ""
        };
        menu.push_str(&format!(
            "{}. {} - 1 Human Adventurer{}\r\n",
            idx, name.0, ld_suffix
        ));
        idx += 1;
    }
    if idx == 1 {
        menu.push_str("You have no characters created yet.\r\n");
    }
    menu.push_str("\r\nc: Create a new character\r\n\r\nWhat would you like to do? ");
    outputs.write(ClientOutput::new(conn, menu));
}

// ─── Command queue dispatch ─────────────────────────────────────────
#[allow(clippy::too_many_arguments)]
fn process_command_queue(
    time: Res<Time>,
    mut clients: Query<(Entity, &mut Client)>,
    mut engine_commands: MessageWriter<EngineCommand>,
    mut announce_logout: MessageWriter<LogoutAnnounce>,
    mut disconnect: MessageWriter<DisconnectRequest>,
    player_chars: Query<(Entity, &GrimName)>,
    characters: Query<&Character>,
    mut commands: Commands,
) {
    for (entity, mut client) in clients.iter_mut() {
        let conn = client.connection;
        if client.state != ClientState::InGame {
            continue;
        }
        client.command_cooldown.tick(time.delta());
        if !client.command_cooldown.just_finished() {
            continue;
        }
        if let Some(cmd) = client.input_queue.pop_front() {
            if matches!(&cmd, Command::Quit) {
                let char_name = client
                    .character
                    .and_then(|c| player_chars.get(c).ok())
                    .map(|(_, n)| n.0.clone())
                    .unwrap_or_else(|| "Someone".into());
                // Save character JSON and despawn
                if let Some(char_entity) = client.character {
                    if let Ok(ch) = characters.get(char_entity) {
                        let path = format!("data/characters/{}.json", ch.name);
                        if let Ok(json) = serde_json::to_string_pretty(ch) {
                            let _ = std::fs::write(path, json);
                        }
                    }
                    commands.entity(char_entity).despawn();
                }
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
        }
    }
}

// ─── Engine → client output formatting ──────────────────────────────

#[allow(clippy::too_many_arguments)]
fn format_output(
    mut look_room_events: MessageReader<LookRoom>,
    mut look_entity_events: MessageReader<LookEntity>,
    mut say_events: MessageReader<SayEvent>,
    mut yell_events: MessageReader<YellEvent>,
    mut ooc_events: MessageReader<OocEvent>,
    mut move_events: MessageReader<MoveEvent>,
    mut info_events: MessageReader<InfoMessage>,
    mut announce_login: MessageReader<LoginAnnounce>,
    mut announce_logout: MessageReader<LogoutAnnounce>,
    mut announce_linkdead: MessageReader<LinkdeadAnnounce>,
    rooms: Query<(Entity, &Room, &GrimName)>,
    room_occupants: Query<(Entity, &InRoom, Option<&Player>, &GrimName)>,
    room_exits: Query<&Exits>,
    names: Query<&GrimName>,
    descriptions: Query<&Description>,
    mut outputs: MessageWriter<ClientOutput>,
) {
    // Helper to find connection from room_occupants
    let find_conn = |target: Entity| -> Entity {
        room_occupants
            .get(target)
            .ok()
            .and_then(|(_, _, p, _)| p.as_ref().and_then(|p| p.connection))
            .unwrap_or(target)
    };
    // ── Login / Logout announces ──
    for ev in announce_login.read() {
        broadcast_global(
            &format!("{} has connected.\r\n", ev.name),
            &room_occupants,
            &mut outputs,
        );
    }
    for ev in announce_logout.read() {
        broadcast_global(
            &format!("{} has disconnected.\r\n", ev.name),
            &room_occupants,
            &mut outputs,
        );
    }

    // ── Linkdead announce ──
    for ev in announce_linkdead.read() {
        let formatted = formatter::format_linkdead(&ev.name, ev.reconnecting);
        broadcast_global(&formatted, &room_occupants, &mut outputs);
    }

    // ── Look room ──
    for ev in look_room_events.read() {
        let Ok((_, room, name)) = rooms.get(ev.room) else {
            continue;
        };
        let exits = room_exits
            .get(ev.room)
            .ok()
            .map(|e| {
                let mut dirs: Vec<String> = e.exits.keys().map(|d| d.to_string()).collect();
                dirs.sort();
                dirs
            })
            .unwrap_or_default();
        let mut occupant_names: Vec<String> = Vec::new();
        for (e, ir, _, occ_name) in room_occupants.iter() {
            if ir.room == ev.room && e != ev.target {
                occupant_names.push(occ_name.0.clone());
            }
        }
        let conn = find_conn(ev.target);
        outputs.write(ClientOutput {
            echo: None,
            ..ClientOutput::new(
                conn,
                formatter::format_room(&name.0, &room.description, &exits, &occupant_names),
            )
        });
    }

    // ── Look entity ──
    for ev in look_entity_events.read() {
        let Ok(subj_name) = names.get(ev.subject) else {
            continue;
        };
        let desc = descriptions
            .get(ev.subject)
            .map(|d| d.0.clone())
            .unwrap_or_default();
        let conn = find_conn(ev.target);
        outputs.write(ClientOutput {
            echo: None,
            ..ClientOutput::new(conn, formatter::format_entity(&subj_name.0, &desc))
        });
    }

    // ── Say ──
    for ev in say_events.read() {
        let Ok(actor_name) = names.get(ev.actor) else {
            continue;
        };
        let formatted = formatter::format_say(&actor_name.0, &ev.text);
        broadcast_to_room(
            ev.room,
            Some(ev.actor),
            &formatted,
            &room_occupants,
            &mut outputs,
        );
    }

    // ── Yell ──
    for ev in yell_events.read() {
        let Ok(actor_name) = names.get(ev.actor) else {
            continue;
        };
        let formatted = formatter::format_yell(&actor_name.0, &ev.text);
        let area_rooms: Vec<Entity> = rooms
            .iter()
            .filter(|(_, r, _)| r.area == ev.area)
            .map(|(e, _, _)| e)
            .collect();
        for (entity, ir, player, _) in room_occupants.iter() {
            if !area_rooms.contains(&ir.room) {
                continue;
            }
            if entity == ev.actor {
                continue;
            }
            if let Some(p) = player {
                if let Some(conn) = p.connection {
                    outputs.write(ClientOutput {
                        prepend_newline: true,
                        ..ClientOutput::new(conn, formatted.clone())
                    });
                }
            }
        }
    }

    // ── Ooc ──
    for ev in ooc_events.read() {
        let Ok(actor_name) = names.get(ev.actor) else {
            continue;
        };
        let formatted = formatter::format_ooc(&actor_name.0, &ev.text);
        for (entity, _, player, _) in room_occupants.iter() {
            if entity == ev.actor {
                continue;
            }
            if let Some(p) = player {
                if let Some(conn) = p.connection {
                    outputs.write(ClientOutput {
                        prepend_newline: true,
                        ..ClientOutput::new(conn, formatted.clone())
                    });
                }
            }
        }
    }

    // ── Move ──
    for ev in move_events.read() {
        let Ok(actor_name) = names.get(ev.actor) else {
            continue;
        };
        let dir_str = ev.direction.to_string();
        let leave_msg = formatter::format_move(&actor_name.0, &dir_str, true);
        broadcast_to_room(
            ev.from,
            Some(ev.actor),
            &leave_msg,
            &room_occupants,
            &mut outputs,
        );
        let arrive_msg = formatter::format_move(&actor_name.0, &dir_str, false);
        broadcast_to_room(
            ev.to,
            Some(ev.actor),
            &arrive_msg,
            &room_occupants,
            &mut outputs,
        );
    }

    // ── InfoMessage ──
    for ev in info_events.read() {
        let conn = find_conn(ev.target);
        outputs.write(ClientOutput {
            prepend_newline: true,
            ..ClientOutput::new(conn, ev.text.clone())
        });
    }
}

/// Capture every `ClientOutput` into the connection's `OutputHistory` for
/// linkdead replay on reconnect.
fn capture_output(
    mut output: MessageReader<ClientOutput>,
    mut histories: Query<&mut OutputHistory>,
) {
    for ev in output.read() {
        if let Ok(mut history) = histories.get_mut(ev.connection) {
            history.push(&ev.text);
        }
    }
}

/// Find the Connection entity for a character, using their Player component.
/// Falls back to the input entity if no Player component found.
#[allow(dead_code)]
fn find_connection(entity: Entity, players: &Query<&Player>) -> Entity {
    players
        .get(entity)
        .ok()
        .and_then(|p| p.connection)
        .unwrap_or(entity)
}

/// Send text to every player in the given room, optionally excluding one entity.
fn broadcast_to_room(
    room: Entity,
    exclude: Option<Entity>,
    text: &str,
    occupants: &Query<(Entity, &InRoom, Option<&Player>, &GrimName)>,
    outputs: &mut MessageWriter<ClientOutput>,
) {
    for (entity, ir, player, _) in occupants.iter() {
        if ir.room != room {
            continue;
        }
        if Some(entity) == exclude {
            continue;
        }
        if let Some(p) = player {
            if let Some(conn) = p.connection {
                outputs.write(ClientOutput {
                    prepend_newline: true,
                    ..ClientOutput::new(conn, text.to_string())
                });
            }
        }
    }
}

/// Send text to every connected player.
fn broadcast_global(
    text: &str,
    occupants: &Query<(Entity, &InRoom, Option<&Player>, &GrimName)>,
    outputs: &mut MessageWriter<ClientOutput>,
) {
    for (_, _, player, _) in occupants.iter() {
        if let Some(p) = player {
            if let Some(conn) = p.connection {
                outputs.write(ClientOutput {
                    prepend_newline: true,
                    ..ClientOutput::new(conn, text.to_string())
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use grim::components::*;
    use grim::plugins::*;
    use std::net::SocketAddr;
    use uuid::Uuid;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(WorldPlugin);
        app.add_plugins(SocialPlugin);
        app.add_plugins(PersistencePlugin);
        app.add_plugins(ClientPlugin);
        // Telnet protocol messages not registered by the above plugins
        app.add_message::<ConnectionEstablished>()
            .add_message::<ClientInput>();
        app
    }

    fn spawn_room(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                Room {
                    id: Uuid::new_v4(),
                    friendly_id: "room1".into(),
                    name: "Room".into(),
                    description: "A room.".into(),
                    area: Entity::PLACEHOLDER,
                },
                GrimName("Room".into()),
            ))
            .id()
    }

    /// Simulate name-based reconnect: type character name at login prompt,
    /// then password. The character has Linkdead — should reconnect.
    #[test]
    fn reconnect_by_name_on_linkdead_character() {
        let mut app = test_app();

        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
                echo_hidden: false,
            })
            .id();

        app.world_mut().spawn(Client::new(conn));

        let account = Account {
            id: Uuid::new_v4(),
            identifier: "test@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![],
            created_at: Utc::now(),
        };
        let account_id = account.id;
        let _account_entity = app.world_mut().spawn(account).id();

        let char_uuid = Uuid::new_v4();
        let character = Character {
            id: char_uuid,
            name: "Test".into(),
            account_id,
            created_at: Utc::now(),
            last_room: None,
        };
        let char_entity = app
            .world_mut()
            .spawn((
                character,
                GrimName("Test".into()),
                Description("A test character.".into()),
                InRoom { room },
                Player { connection: None },
                Linkdead,
                OutputHistory::with_max(100),
            ))
            .id();

        // Step 1: Send character name at login prompt
        app.world_mut().write_message(ClientInput {
            connection: conn,
            text: "Test".into(),
        });
        app.update();

        // Check: client should now be in PasswordPrompt state
        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(&app.world()).find(|(_, c)| c.connection == conn);
        assert!(found.is_some(), "Client should exist");
        let (_client_entity, client) = found.unwrap();
        assert_eq!(
            client.state,
            ClientState::PasswordPrompt {
                identifier: "test@example.com".into(),
                is_new: false,
                character: Some(char_entity),
            },
            "Should be in PasswordPrompt state after name entry"
        );

        // Step 2: Send password
        app.world_mut().write_message(ClientInput {
            connection: conn,
            text: "password".into(),
        });
        app.update();

        // Check: Linkdead should be removed, Player.connection should be Some
        let mut players = app.world_mut().query::<&Player>();
        let player = players.get(&app.world(), char_entity);
        assert!(player.is_ok(), "Character should have Player component");
        assert!(
            player.unwrap().connection.is_some(),
            "Player should be connected after reconnect"
        );

        let mut linkdead = app.world_mut().query::<&Linkdead>();
        assert!(
            linkdead.get(&app.world(), char_entity).is_err(),
            "Linkdead should be removed after reconnect"
        );

        // Check: LinkdeadAnnounce was written with reconnecting: true
        let msg_resource = app.world().resource::<Messages<LinkdeadAnnounce>>();
        let mut cursor = msg_resource.get_cursor();
        let announces: Vec<&LinkdeadAnnounce> = cursor.read(&msg_resource).collect();
        let has_reconnect = announces.iter().any(|a| a.reconnecting && a.name == "Test");
        assert!(
            has_reconnect,
            "Should emit LinkdeadAnnounce with reconnecting=true"
        );
    }

    /// Simulate email-based reconnect: type email, then password, then select
    /// character from menu. Character has Linkdead — should reconnect.
    #[test]
    fn reconnect_by_email_on_linkdead_character() {
        let mut app = test_app();

        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
                echo_hidden: false,
            })
            .id();

        app.world_mut().spawn(Client::new(conn));

        let char_uuid = Uuid::new_v4();
        let account = Account {
            id: Uuid::new_v4(),
            identifier: "test@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![char_uuid],
            created_at: Utc::now(),
        };
        let account_id = account.id;
        app.world_mut().spawn(account);

        let character = Character {
            id: char_uuid,
            name: "Test".into(),
            account_id,
            created_at: Utc::now(),
            last_room: None,
        };
        let char_entity = app
            .world_mut()
            .spawn((
                character,
                GrimName("Test".into()),
                Description("A test character.".into()),
                InRoom { room },
                Player { connection: None },
                Linkdead,
                OutputHistory::with_max(100),
            ))
            .id();

        // Step 1: Send email at login prompt
        app.world_mut().write_message(ClientInput {
            connection: conn,
            text: "test@example.com".into(),
        });
        app.update();

        // Check: client should now be in PasswordPrompt state
        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(&app.world()).find(|(_, c)| c.connection == conn);
        assert!(found.is_some(), "Client should exist");
        let (_client_entity, client) = found.unwrap();
        assert_eq!(
            client.state,
            ClientState::PasswordPrompt {
                identifier: "test@example.com".into(),
                is_new: false,
                character: None,
            },
            "Should be in PasswordPrompt state after email entry"
        );

        // Step 2: Send password
        app.world_mut().write_message(ClientInput {
            connection: conn,
            text: "password".into(),
        });
        app.update();

        // Check: client should now be in CharacterSelect state
        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(&app.world()).find(|(_, c)| c.connection == conn);
        assert!(found.is_some(), "Client should exist");
        let (_client_entity, client) = found.unwrap();
        assert_eq!(
            client.state,
            ClientState::CharacterSelect,
            "Should be in CharacterSelect after password for email login"
        );

        // Step 3: Select character by number
        app.world_mut().write_message(ClientInput {
            connection: conn,
            text: "1".into(),
        });
        app.update();

        // Check: Linkdead should be removed, Player.connection should be Some
        let mut players = app.world_mut().query::<&Player>();
        let player = players.get(&app.world(), char_entity);
        assert!(player.is_ok(), "Character should have Player component");
        assert!(
            player.unwrap().connection.is_some(),
            "Player should be connected after reconnect"
        );

        let mut linkdead = app.world_mut().query::<&Linkdead>();
        assert!(
            linkdead.get(&app.world(), char_entity).is_err(),
            "Linkdead should be removed after reconnect"
        );
        // Check: LinkdeadAnnounce was written with reconnecting: true
        let msg_resource = app.world().resource::<Messages<LinkdeadAnnounce>>();
        let mut cursor = msg_resource.get_cursor();
        let announces: Vec<&LinkdeadAnnounce> = cursor.read(&msg_resource).collect();
        let has_reconnect = announces.iter().any(|a| a.reconnecting && a.name == "Test");
        assert!(
            has_reconnect,
            "Should emit LinkdeadAnnounce with reconnecting=true"
        );
    }

    /// Reproduce the duplicate-entity bug: two character entities with the same
    /// name exist (one with Linkdead, one freshly loaded from disk without).
    /// The name-based login should find the linkdead one, but the query finds
    /// the first match which may be the wrong entity.
    #[test]
    fn duplicate_entity_name_login_finds_wrong_one() {
        let mut app = test_app();

        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
                echo_hidden: false,
            })
            .id();
        app.world_mut().spawn(Client::new(conn));

        let account_uuid = Uuid::new_v4();
        let account = Account {
            id: account_uuid,
            identifier: "test@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![],
            created_at: Utc::now(),
        };
        app.world_mut().spawn(account);

        // Entity A: stale — loaded from disk, no Linkdead
        let stale_uuid = Uuid::new_v4();
        app.world_mut().spawn((
            Character {
                id: stale_uuid,
                name: "Test".into(),
                account_id: account_uuid,
                created_at: Utc::now(),
                last_room: None,
            },
            GrimName("Test".into()),
            Description("Stale copy.".into()),
        ));

        // Entity B: real — in-world, went linkdead
        let real_entity = app
            .world_mut()
            .spawn((
                Character {
                    id: Uuid::new_v4(),
                    name: "Test".into(),
                    account_id: account_uuid,
                    created_at: Utc::now(),
                    last_room: None,
                },
                GrimName("Test".into()),
                Description("Real character.".into()),
                InRoom { room },
                Player { connection: None },
                Linkdead,
                OutputHistory::with_max(100),
            ))
            .id();

        // Send character name at login prompt
        app.world_mut().write_message(ClientInput {
            connection: conn,
            text: "Test".into(),
        });
        app.update();

        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(&app.world()).find(|(_, c)| c.connection == conn);
        assert!(found.is_some(), "Client should exist");
        let (_client_entity, client) = found.unwrap();

        match &client.state {
            ClientState::PasswordPrompt { character, .. } => {
                let selected = character.expect("Should have auto-selected a character");
                assert_eq!(
                    selected, real_entity,
                    "Should have found the linkdead entity, not the stale one"
                );
            }
            other => panic!("Expected PasswordPrompt, got {:?}", other),
        }
    }

    /// Duplicate entity via email login + character select menu.
    #[test]
    fn duplicate_entity_email_login_finds_wrong_one() {
        let mut app = test_app();

        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
                echo_hidden: false,
            })
            .id();
        app.world_mut().spawn(Client::new(conn));

        let account_uuid = Uuid::new_v4();
        let real_uuid = Uuid::new_v4();
        let stale_uuid = Uuid::new_v4();
        let account = Account {
            id: account_uuid,
            identifier: "test@example.com".into(),
            password_hash: hash_password("password"),
            characters: vec![real_uuid, stale_uuid],
            created_at: Utc::now(),
        };
        app.world_mut().spawn(account);

        // Entity A: stale (no Linkdead)
        app.world_mut().spawn((
            Character {
                id: stale_uuid,
                name: "Test".into(),
                account_id: account_uuid,
                created_at: Utc::now(),
                last_room: None,
            },
            GrimName("Test".into()),
            Description("Stale copy.".into()),
        ));

        // Entity B: real (with Linkdead)
        let real_entity = app
            .world_mut()
            .spawn((
                Character {
                    id: real_uuid,
                    name: "Test".into(),
                    account_id: account_uuid,
                    created_at: Utc::now(),
                    last_room: None,
                },
                GrimName("Test".into()),
                Description("Real character.".into()),
                InRoom { room },
                Player { connection: None },
                Linkdead,
                OutputHistory::with_max(100),
            ))
            .id();

        // Step 1: Send email
        app.world_mut().write_message(ClientInput {
            connection: conn,
            text: "test@example.com".into(),
        });
        app.update();

        // Step 2: Send password
        app.world_mut().write_message(ClientInput {
            connection: conn,
            text: "password".into(),
        });
        app.update();

        let mut query = app.world_mut().query::<(Entity, &Client)>();
        let found = query.iter(&app.world()).find(|(_, c)| c.connection == conn);
        assert!(found.is_some(), "Client should exist");
        let (_client_entity, client) = found.unwrap();
        assert_eq!(
            client.state,
            ClientState::CharacterSelect,
            "Should be in CharacterSelect after email login"
        );

        // Step 3: Select character "1"
        app.world_mut().write_message(ClientInput {
            connection: conn,
            text: "1".into(),
        });
        app.update();

        let mut players = app.world_mut().query::<&Player>();
        let player = players.get(&app.world(), real_entity);
        assert!(player.is_ok(), "Character should have Player component");
        assert!(
            player.unwrap().connection.is_some(),
            "Player should be connected after reconnect"
        );

        let mut linkdead = app.world_mut().query::<&Linkdead>();
        assert!(
            linkdead.get(&app.world(), real_entity).is_err(),
            "Linkdead should be removed after reconnect"
        );
    }

    /// Test that WITHOUT ordering, the first input is lost because the Client
    /// entity is spawned via deferred commands. This test manually adds the
    /// systems without ordering to demonstrate the problem.
    #[test]
    fn first_input_lost_without_ordering() {
        use grim::plugins::*;
        use std::net::SocketAddr;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(WorldPlugin);
        app.add_plugins(SocialPlugin);
        app.add_plugins(PersistencePlugin);
        app.add_message::<ClientOutput>()
            .add_message::<DisconnectRequest>()
            .add_message::<EngineCommand>()
            .add_message::<LookRoom>()
            .add_message::<LookEntity>()
            .add_message::<SayEvent>()
            .add_message::<YellEvent>()
            .add_message::<OocEvent>()
            .add_message::<MoveEvent>()
            .add_message::<InfoMessage>()
            .add_message::<LoginAnnounce>()
            .add_message::<LogoutAnnounce>()
            .add_message::<LinkdeadAnnounce>()
            .add_message::<ConnectionEstablished>()
            .add_message::<ClientInput>()
            .add_systems(Update, handle_connection_established)
            .add_systems(Update, handle_client_input);

        let room = app
            .world_mut()
            .spawn((
                Room {
                    id: Uuid::new_v4(),
                    friendly_id: "room1".into(),
                    name: "Room".into(),
                    description: "A room.".into(),
                    area: Entity::PLACEHOLDER,
                },
                GrimName("Room".into()),
            ))
            .id();
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
                echo_hidden: false,
            })
            .id();

        app.world_mut().write_message(ConnectionEstablished {
            connection: conn,
            addr: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
        });
        app.world_mut().write_message(ClientInput {
            connection: conn,
            text: "Test".into(),
        });

        app.update();

        let mut client_query = app.world_mut().query::<&Client>();
        let client_count = client_query.iter(&app.world()).len();
        assert_eq!(client_count, 1, "Client should have been spawned");

        let mut client_state_query = app.world_mut().query::<&Client>();
        let client = client_state_query.iter(&app.world()).next().unwrap();
        assert_eq!(
            client.state,
            ClientState::LoginPrompt,
            "First input should be lost: client should still be in LoginPrompt"
        );

        app.update();
        let mut client_state_query2 = app.world_mut().query::<&Client>();
        let client2 = client_state_query2.iter(&app.world()).next().unwrap();
        assert_eq!(
            client2.state,
            ClientState::LoginPrompt,
            "Input was permanently lost: should still be in LoginPrompt"
        );
    }

    /// Verify that format_output broadcasts SayEvent to room occupants.
    #[test]
    fn format_output_say_broadcast() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let actor_conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        let observer_conn = app
            .world_mut()
            .spawn(Connection {
                id: 2,
                addr: "127.0.0.1:12346".parse().unwrap(),
                echo_hidden: false,
            })
            .id();

        let actor = app
            .world_mut()
            .spawn((
                GrimName("Hero".into()),
                InRoom { room },
                Player {
                    connection: Some(actor_conn),
                },
                OutputHistory::with_max(100),
            ))
            .id();
        let _observer = app
            .world_mut()
            .spawn((
                GrimName("Bystander".into()),
                InRoom { room },
                Player {
                    connection: Some(observer_conn),
                },
                OutputHistory::with_max(100),
            ))
            .id();

        app.world_mut().write_message(SayEvent {
            room,
            actor,
            text: "hello".into(),
        });
        app.world_mut().write_message(InfoMessage {
            target: actor,
            text: "You say, 'hello'\r\n".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ClientOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ClientOutput> = cursor.read(&msgs).collect();

        assert!(
            outputs
                .iter()
                .any(|o| o.connection == observer_conn && o.text.contains("Hero says")),
            "observer should get broadcast"
        );
        assert!(
            outputs
                .iter()
                .any(|o| o.connection == actor_conn && o.text.contains("You say")),
            "actor should get echo"
        );
    }

    /// Verify that format_output handles LoginAnnounce (broadcast_global path).
    #[test]
    fn format_output_login_announce() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        app.world_mut().spawn((
            GrimName("Hero".into()),
            InRoom { room },
            Player {
                connection: Some(conn),
            },
            OutputHistory::with_max(100),
        ));

        app.world_mut().write_message(LoginAnnounce {
            name: "Hero".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ClientOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ClientOutput> = cursor.read(&msgs).collect();
        assert!(
            outputs
                .iter()
                .any(|o| o.text.contains("Hero has connected")),
            "should announce login"
        );
    }

    /// Verify that format_output handles LogoutAnnounce.
    #[test]
    fn format_output_logout_announce() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        app.world_mut().spawn((
            GrimName("Hero".into()),
            InRoom { room },
            Player {
                connection: Some(conn),
            },
            OutputHistory::with_max(100),
        ));

        app.world_mut().write_message(LogoutAnnounce {
            name: "Hero".into(),
        });
        app.update();

        let msgs = app.world().resource::<Messages<ClientOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ClientOutput> = cursor.read(&msgs).collect();
        assert!(
            outputs
                .iter()
                .any(|o| o.text.contains("Hero has disconnected")),
            "should announce logout"
        );
    }

    /// Verify that format_output handles LinkdeadAnnounce (reconnecting).
    #[test]
    fn format_output_linkdead_announce() {
        let mut app = test_app();
        let room = spawn_room(&mut app);
        app.world_mut().insert_resource(StartingRoom(room));

        let conn = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:12345".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        app.world_mut().spawn((
            GrimName("Hero".into()),
            InRoom { room },
            Player {
                connection: Some(conn),
            },
            OutputHistory::with_max(100),
        ));

        app.world_mut().write_message(LinkdeadAnnounce {
            name: "Hero".into(),
            reconnecting: true,
        });
        app.update();

        let msgs = app.world().resource::<Messages<ClientOutput>>();
        let mut cursor = msgs.get_cursor();
        let outputs: Vec<&ClientOutput> = cursor.read(&msgs).collect();
        assert!(
            outputs
                .iter()
                .any(|o| o.text.contains("Hero has reconnected")),
            "should announce reconnect"
        );
    }
}
