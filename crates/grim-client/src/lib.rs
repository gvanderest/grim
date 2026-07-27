use std::collections::VecDeque;
use std::time::Duration;

use bevy::log::info;
use bevy::prelude::{
    App, Commands, Entity, MessageReader, MessageWriter, Plugin, Query, Res, Time, Timer,
    TimerMode, Update,
};
use chrono::Utc;
use uuid::Uuid;

use grim::components::{
    Account, Area, Character, Client, ClientState, Description, Exits, InRoom, Linkdead,
    Name as GrimName, OutputHistory, Player, Room, StartingRoom,
};
use grim::events::*;
use grim::validation::{
    hash_password, validate_character_name, validate_identifier, validate_password, verify_password,
};

mod formatter;
mod parser;

pub struct ClientPlugin;
impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
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
            .add_systems(Update, handle_connection_established)
            .add_systems(Update, handle_client_input)
            .add_systems(Update, process_command_queue)
            .add_systems(Update, format_output)
            .add_systems(Update, capture_output);
    }
}

// ─── Connection lifecycle ───────────────────────────────────────────

fn handle_connection_established(
    mut established: MessageReader<ConnectionEstablished>,
    mut commands: Commands,
    mut outputs: MessageWriter<ClientOutput>,
) {
    for ev in established.read() {
        commands.spawn(Client::new(ev.connection));
        commands
            .entity(ev.connection)
            .insert(OutputHistory::with_max(100));
        outputs.write(ClientOutput {
            connection: ev.connection,
            text: format!(
                "{}\r\n\r\nEnter your email address: ",
                include_str!("../../../assets/login-banner.txt")
            ),
            echo: None,
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
    _histories: Query<&mut OutputHistory>,
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
                        connection: conn,
                        text: "Enter your email address: ".into(),
                        echo: None,
                    });
                    continue;
                }
                // First, try as character name
                let trimmed = text.trim();
                let char_match = characters
                    .iter()
                    .find(|(_, _, n)| n.0.eq_ignore_ascii_case(trimmed));
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
                            connection: conn,
                            text: "Password: ".into(),
                            echo: Some(false),
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
                                connection: conn,
                                text: "Password: ".into(),
                                echo: Some(false),
                            });
                        } else {
                            client.state = ClientState::ConfirmCreate {
                                identifier: identifier.clone(),
                            };
                            outputs.write(ClientOutput {
                                connection: conn,
                                text: "Did not find that email address, do you want to create an account? [Y/n] ".into(),
                                echo: None,
                            });
                        }
                    }
                    Err(e) => {
                        outputs.write(ClientOutput {
                            connection: conn,
                            text: format!(
                                "Invalid identifier: {}\r\nEnter your email address or character name: ",
                                e
                            ),
                            echo: None,
                        });
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
                        connection: conn,
                        text: "Choose a password: ".into(),
                        echo: Some(false),
                    });
                } else {
                    client.state = ClientState::LoginPrompt;
                    outputs.write(ClientOutput {
                        connection: conn,
                        text: "Enter your email address: ".into(),
                        echo: None,
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
                        connection: conn,
                        text: "Invalid password.\r\nEnter your email address: ".into(),
                        echo: Some(true),
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
                                connection: conn,
                                text: "".into(),
                                echo: Some(true),
                            });
                            show_character_menu(
                                client_entity,
                                &client,
                                &characters,
                                &accounts,
                                &mut outputs,
                                &linkdead,
                                None,
                            );
                        }
                        Err(e) => {
                            outputs.write(ClientOutput {
                                connection: conn,
                                text: format!("Invalid password: {}\r\nChoose a password: ", e),
                                echo: None,
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
                                    if linkdead.get(char_entity).is_ok() {
                                        // Linkdead reconnect
                                        commands.entity(char_entity).remove::<Linkdead>();
                                        commands
                                            .entity(char_entity)
                                            .insert(Player { connection: conn });
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
                                            connection: conn,
                                            text: "Reconnecting...\r\n".into(),
                                            echo: None,
                                        });
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
                                    } else {
                                        commands.entity(char_entity).insert((
                                            Player { connection: conn },
                                            InRoom { room: starting.0 },
                                        ));
                                        client.character = Some(char_entity);
                                        client.state = ClientState::MotdPrompt;
                                        outputs.write(ClientOutput {
                                            connection: conn,
                                            text: formatter::format_motd(),
                                            echo: Some(true),
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
                                        Some(true),
                                    );
                                }
                            } else {
                                outputs.write(ClientOutput {
                                    connection: conn,
                                    text: "Invalid password.\r\nPassword: ".into(),
                                    echo: None,
                                });
                            }
                        }
                        None => {
                            client.state = ClientState::LoginPrompt;
                            outputs.write(ClientOutput {
                                connection: conn,
                                text: "Account not found.\r\nEnter your email address: ".into(),
                                echo: Some(true),
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
                        connection: conn,
                        text: "Enter a name for your new character: ".into(),
                        echo: None,
                    });
                    continue;
                }

                let Some(account_entity) = client.account else {
                    continue;
                };
                let Ok((_, account)) = accounts.get(account_entity) else {
                    continue;
                };
                let char_list: Vec<(Entity, Uuid, String)> = characters
                    .iter()
                    .filter(|(_, c, _)| account.characters.contains(&c.id))
                    .map(|(e, c, n)| (e, c.id, n.0.clone()))
                    .collect();

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
                        None,
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
                    commands
                        .entity(char_entity)
                        .insert(Player { connection: conn });
                    client.character = Some(char_entity);
                    client.state = ClientState::InGame;
                    client.input_queue = VecDeque::new();
                    client.command_cooldown = {
                        let mut t = Timer::new(Duration::from_millis(10), TimerMode::Repeating);
                        t.set_elapsed(Duration::from_millis(10));
                        t
                    };
                    outputs.write(ClientOutput {
                        connection: conn,
                        text: "Reconnecting...\r\n".into(),
                        echo: None,
                    });
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
                } else {
                    commands
                        .entity(char_entity)
                        .insert((Player { connection: conn }, InRoom { room: starting.0 }));
                    client.character = Some(char_entity);
                    client.state = ClientState::MotdPrompt;
                    outputs.write(ClientOutput {
                        connection: conn,
                        text: formatter::format_motd(),
                        echo: None,
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
                        let path = format!("data/characters/{}.json", char_id);
                        if let Ok(json) = serde_json::to_string_pretty(&character) {
                            let _ = std::fs::write(path, json);
                        }
                        let char_entity = commands
                            .spawn((
                                character,
                                GrimName(name.clone()),
                                Description("A new adventurer.".into()),
                                Player { connection: conn },
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
                            connection: conn,
                            text: formatter::format_motd(),
                            echo: None,
                        });
                    }
                    Err(e) => {
                        outputs.write(ClientOutput {
                            connection: conn,
                            text: format!(
                                "Invalid name: {}\r\nEnter a name for your character: ",
                                e
                            ),
                            echo: None,
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
                            let mut names: Vec<String> = player_chars
                                .iter()
                                .filter(|(_, _, _, c)| c.is_some())
                                .map(|(_, n, _, _)| n.0.clone())
                                .collect();
                            names.sort();
                            outputs.write(ClientOutput {
                                connection: conn,
                                text: formatter::format_who_list(&names),
                                echo: None,
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
                                connection: conn,
                                text: formatter::format_where_list(&entries),
                                echo: None,
                            });
                        }
                        Command::Commands => {
                            outputs.write(ClientOutput {
                                connection: conn,
                                text: formatter::format_commands(),
                                echo: None,
                            });
                        }
                        _ => {
                            client.input_queue.push_back(cmd);
                        }
                    }
                } else if text.trim().is_empty() {
                    // Blank line — just force a prompt on next flush
                    outputs.write(ClientOutput {
                        connection: conn,
                        text: "".into(),
                        echo: None,
                    });
                } else {
                    outputs.write(ClientOutput {
                        connection: conn,
                        text: "Unknown command. Type 'commands' for a list.\r\n".into(),
                        echo: None,
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
    echo: Option<bool>,
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
    outputs.write(ClientOutput {
        connection: conn,
        text: menu,
        echo,
    });
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
                        let path = format!("data/characters/{}.json", ch.id);
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
            .and_then(|(_, _, p, _)| p.as_ref().map(|p| p.connection))
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
            connection: conn,
            text: formatter::format_room(&name.0, &room.description, &exits, &occupant_names),
            echo: None,
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
            connection: conn,
            text: formatter::format_entity(&subj_name.0, &desc),
            echo: None,
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
                outputs.write(ClientOutput {
                    connection: p.connection,
                    text: formatted.clone(),
                    echo: None,
                });
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
                outputs.write(ClientOutput {
                    connection: p.connection,
                    text: formatted.clone(),
                    echo: None,
                });
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
            connection: conn,
            text: ev.text.clone(),
            echo: None,
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
    players.get(entity).map(|p| p.connection).unwrap_or(entity)
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
            outputs.write(ClientOutput {
                connection: p.connection,
                text: text.to_string(),
                echo: None,
            });
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
            outputs.write(ClientOutput {
                connection: p.connection,
                text: text.to_string(),
                echo: None,
            });
        }
    }
}
