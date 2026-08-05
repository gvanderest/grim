//! The tokio↔Bevy bridge: the channel-joined connection registry, the internal
//! event/command types crossing the seam, the per-socket read/write task split
//! (`register_connection`), and the two Bevy systems that drain network events
//! into messages and route outbound messages back to the network.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};

use bevy::log::info;
use bevy::prelude::*;
use grim_engine_types::components::{Client, ClientState};
use grim_networking::{
    Connection, ConnectionClosed, ConnectionEstablished, ConnectionInput, ConnectionOutput,
    ConnectionResumed, DisconnectRequest,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::{iac, render};

// ─── Internal bridge types ─────────────────────────────────────────

pub(crate) struct Conn {
    pub(crate) write_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    /// Both task handles are aborted on disconnect so *both* halves of the split
    /// socket drop and the fd actually closes. Dropping `write_tx` alone does not
    /// reliably end the write task across a chained copyover, which left the
    /// socket open (the client never saw EOF on `quit`).
    pub(crate) read_handle: tokio::task::JoinHandle<()>,
    pub(crate) write_handle: tokio::task::JoinHandle<()>,
    /// Raw fd of the underlying socket, recorded at accept/adopt time. Stays
    /// valid while the socket lives; sent (dup'd) to the successor on copyover.
    pub(crate) raw_fd: RawFd,
}

pub(crate) enum NetworkEvent {
    Connected {
        conn_id: usize,
        addr: SocketAddr,
    },
    /// A socket re-adopted from a predecessor across a copyover. Unlike
    /// `Connected`, the session layer resumes `character` straight into the world.
    Resumed {
        conn_id: usize,
        addr: SocketAddr,
        character: String,
        echo_hidden: bool,
    },
    Input {
        conn_id: usize,
        text: String,
    },
    Disconnected {
        conn_id: usize,
    },
}

pub(crate) enum NetworkCommand {
    Send {
        conn_id: usize,
        text: String,
    },
    SendRaw {
        conn_id: usize,
        data: Vec<u8>,
    },
    Disconnect {
        conn_id: usize,
    },
    /// Begin a copyover: hand the listed in-game connections (and the listener)
    /// to a freshly-spawned successor process, then let the app exit.
    Copyover {
        conns: Vec<CopyoverConn>,
    },
}

/// One in-game connection selected for copyover handoff, joined transport-side
/// with its raw fd by `conn_id`.
pub(crate) struct CopyoverConn {
    pub(crate) conn_id: usize,
    pub(crate) character: String,
    pub(crate) echo_hidden: bool,
}

#[derive(Resource)]
pub(crate) struct NetworkBridge {
    pub(crate) to_network: tokio::sync::mpsc::Sender<NetworkCommand>,
    pub(crate) from_network: Arc<Mutex<std_mpsc::Receiver<NetworkEvent>>>,
}

#[derive(Resource)]
pub(crate) struct TelnetPort(pub u16);

// ─── Connection registration ───────────────────────────────────────

/// Split `socket` into read/write tasks and register it under `conn_id`. Fresh
/// accepts send the minimal telnet negotiation first (`handshake`); re-adopted
/// copyover sockets have already negotiated, so they skip it.
pub(crate) fn register_connection(
    conn_id: usize,
    socket: TcpStream,
    to_bevy_tx: &std_mpsc::Sender<NetworkEvent>,
    conns: &Arc<Mutex<HashMap<usize, Conn>>>,
    handshake: bool,
) {
    let raw_fd = socket.as_raw_fd();
    let (read_half, mut write_half) = tokio::io::split(socket);
    let (write_tx, mut write_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(32);

    let read_handle = tokio::spawn({
        let to_bevy_tx = to_bevy_tx.clone();
        let conns = conns.clone();
        async move {
            let mut reader = BufReader::new(read_half);
            let mut buf: Vec<u8> = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf).await {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
                let clean = iac::strip_iac(&buf);
                let text = String::from_utf8_lossy(&clean)
                    .trim_end_matches(['\r', '\n'])
                    .to_string();
                let _ = to_bevy_tx.send(NetworkEvent::Input { conn_id, text });
            }
            let _ = to_bevy_tx.send(NetworkEvent::Disconnected { conn_id });
            conns.lock().unwrap().remove(&conn_id);
        }
    });

    let write_handle = tokio::spawn(async move {
        if handshake {
            // IAC WILL ECHO, IAC WILL SUPPRESS_GO_AHEAD.
            let _ = write_half.write_all(&iac::HANDSHAKE).await;
        }
        while let Some(data) = write_rx.recv().await {
            if write_half.write_all(&data).await.is_err() {
                break;
            }
        }
    });

    conns.lock().unwrap().insert(
        conn_id,
        Conn {
            write_tx,
            read_handle,
            write_handle,
            raw_fd,
        },
    );
}

// ─── Update: drain network -> Bevy events ──────────────────────────

pub(crate) fn drain_network_events(
    bridge: Res<NetworkBridge>,
    mut commands: Commands,
    mut established: MessageWriter<ConnectionEstablished>,
    mut resumed: MessageWriter<ConnectionResumed>,
    mut input: MessageWriter<ConnectionInput>,
    mut closed: MessageWriter<ConnectionClosed>,
    mut connections: Query<(Entity, &mut Connection)>,
) {
    let rx = bridge.from_network.lock().unwrap();
    while let Ok(ev) = rx.try_recv() {
        match ev {
            NetworkEvent::Connected { conn_id, addr } => {
                info!("Connection from {} (id={})", addr, conn_id);
                let entity = commands
                    .spawn(Connection {
                        id: conn_id,
                        addr,
                        echo_hidden: false,
                    })
                    .id();
                established.write(ConnectionEstablished {
                    connection: entity,
                    addr,
                });
            }
            NetworkEvent::Resumed {
                conn_id,
                addr,
                character,
                echo_hidden,
            } => {
                info!(
                    "Connection {} resumed as '{}' (copyover)",
                    conn_id, character
                );
                let entity = commands
                    .spawn(Connection {
                        id: conn_id,
                        addr,
                        echo_hidden,
                    })
                    .id();
                // No banner, no login: the session layer places the character
                // straight back into the world.
                resumed.write(ConnectionResumed {
                    connection: entity,
                    character,
                });
            }
            NetworkEvent::Input { conn_id, text } => {
                if let Some((entity, mut conn)) =
                    connections.iter_mut().find(|(_, c)| c.id == conn_id)
                {
                    // Filter to printable ASCII (32-126) — strip ANSI/control chars
                    let text: String = text
                        .chars()
                        .filter(|&c| c.is_ascii_graphic() || c == ' ')
                        .collect();

                    // If echo was hidden (password mode), auto-restore on user input.
                    if conn.echo_hidden {
                        let _ = bridge.to_network.try_send(NetworkCommand::SendRaw {
                            conn_id,
                            data: iac::WONT_ECHO.to_vec(), // IAC WONT ECHO → visible
                        });
                        let _ = bridge.to_network.try_send(NetworkCommand::Send {
                            conn_id,
                            text: "\n".into(),
                        });
                        conn.echo_hidden = false;
                    }
                    input.write(ConnectionInput {
                        connection: entity,
                        text,
                    });
                }
            }
            NetworkEvent::Disconnected { conn_id } => {
                info!("Connection {} disconnected", conn_id);
                if let Some((entity, _)) = connections.iter().find(|(_, c)| c.id == conn_id) {
                    closed.write(ConnectionClosed { connection: entity });
                    // Despawn is handled by save_on_disconnect in the persistence plugin
                }
            }
        }
    }
}

// ─── Update: route Bevy events -> network ──────────────────────────

pub(crate) fn send_network_commands(
    bridge: Res<NetworkBridge>,
    mut output: MessageReader<ConnectionOutput>,
    mut disconnect: MessageReader<DisconnectRequest>,
    mut connections: Query<&mut Connection>,
    clients: Query<&Client>,
) {
    for ev in output.read() {
        if let Ok(mut conn) = connections.get_mut(ev.connection) {
            if let Some(echo_state) = ev.echo {
                let data = if echo_state {
                    iac::WONT_ECHO.to_vec() // IAC WONT ECHO → visible input (normal)
                } else {
                    iac::WILL_ECHO.to_vec() // IAC WILL ECHO → hidden input (password)
                };
                let _ = bridge.to_network.try_send(NetworkCommand::SendRaw {
                    conn_id: conn.id,
                    data,
                });
                conn.echo_hidden = !echo_state;
                if echo_state {
                    let _ = bridge.to_network.try_send(NetworkCommand::Send {
                        conn_id: conn.id,
                        text: "\n".into(),
                    });
                }
            }
            let is_ingame = clients
                .iter()
                .any(|c| c.state == ClientState::InGame && c.connection == ev.connection);
            let ready = render::render_output(&ev.text, is_ingame, ev.prepend_newline);
            let _ = bridge.to_network.try_send(NetworkCommand::Send {
                conn_id: conn.id,
                text: ready,
            });
        }
    }
    for ev in disconnect.read() {
        if let Ok(conn) = connections.get(ev.connection) {
            let _ = bridge
                .to_network
                .try_send(NetworkCommand::Disconnect { conn_id: conn.id });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::TelnetPlugin;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    /// Register the transport messages a bare `App` needs before driving the plugin.
    fn add_messages(app: &mut App) {
        app.add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<DisconnectRequest>();
    }

    /// Accept loop: a fresh connection produces `ConnectionEstablished`, input
    /// round-trips to `ConnectionInput`, and a dropped socket yields `ConnectionClosed`.
    mod accept {
        use super::*;

        #[test]
        fn telnet_plugin_accepts_connection() {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(TelnetPlugin { port: 19999 });
            add_messages(&mut app);

            app.update();

            std::thread::sleep(Duration::from_millis(100));

            let mut stream = TcpStream::connect_timeout(
                &"127.0.0.1:19999".parse().unwrap(),
                Duration::from_secs(2),
            )
            .expect("should connect to telnet server");

            let mut handshake = [0u8; 6];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let _ = stream.read(&mut handshake);

            app.update();

            {
                let msg_resource = app.world().resource::<Messages<ConnectionEstablished>>();
                let mut cursor = msg_resource.get_cursor();
                let events: Vec<&ConnectionEstablished> = cursor.read(msg_resource).collect();
                assert!(
                    !events.is_empty(),
                    "Should have received ConnectionEstablished"
                );
            }

            stream.write_all(b"hello\n").ok();
            std::thread::sleep(Duration::from_millis(50));

            app.update();

            {
                let msg_resource = app.world().resource::<Messages<ConnectionInput>>();
                let mut cursor = msg_resource.get_cursor();
                let events: Vec<&ConnectionInput> = cursor.read(msg_resource).collect();
                let has_hello = events.iter().any(|e| e.text == "hello");
                assert!(has_hello, "Should have received 'hello' ConnectionInput");
            }

            drop(stream);
            std::thread::sleep(Duration::from_millis(100));
            app.update();

            {
                let msg_resource = app.world().resource::<Messages<ConnectionClosed>>();
                let mut cursor = msg_resource.get_cursor();
                let events: Vec<&ConnectionClosed> = cursor.read(msg_resource).collect();
                assert!(!events.is_empty(), "Should have received ConnectionClosed");
            }
        }

        #[test]
        fn input_filter_strips_control_chars() {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(TelnetPlugin { port: 19993 });
            add_messages(&mut app);

            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut stream = TcpStream::connect_timeout(
                &"127.0.0.1:19993".parse().unwrap(),
                Duration::from_secs(2),
            )
            .expect("should connect");

            let mut handshake = [0u8; 6];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let _ = stream.read(&mut handshake);

            app.update();
            std::thread::sleep(Duration::from_millis(50));

            // Send input with control chars: tab (0x09), escape (0x1B), bell (0x07), and regular text
            let raw = b"\x07\x1Bhello\x09world\n".to_vec();
            stream.write_all(&raw).ok();
            std::thread::sleep(Duration::from_millis(50));

            app.update();

            let msg_resource = app.world().resource::<Messages<ConnectionInput>>();
            let mut cursor = msg_resource.get_cursor();
            let events: Vec<&ConnectionInput> = cursor.read(msg_resource).collect();
            let has_filtered = events.iter().any(|e| e.text == "helloworld");
            assert!(
                has_filtered,
                "Control chars should be stripped, got: {:?}",
                events.iter().map(|e| &e.text).collect::<Vec<_>>()
            );

            let has_raw = events.iter().any(|e| e.text.contains('\t'));
            assert!(!has_raw, "Tab should be stripped");

            drop(stream);
        }
    }

    /// Echo negotiation: `echo: Some(false)` hides input (`IAC WILL ECHO`) and the
    /// next user input auto-restores it (`IAC WONT ECHO`).
    mod echo {
        use super::*;

        #[test]
        fn echo_false_sets_echo_hidden() {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(TelnetPlugin { port: 19995 });
            add_messages(&mut app);

            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut stream = TcpStream::connect_timeout(
                &"127.0.0.1:19995".parse().unwrap(),
                Duration::from_secs(2),
            )
            .expect("should connect");

            let mut handshake = [0u8; 6];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let _ = stream.read(&mut handshake);

            app.update();

            let mut query = app.world_mut().query::<(Entity, &Connection)>();
            let (conn_entity, conn) = query
                .iter(app.world())
                .next()
                .expect("should have a connection");
            assert!(!conn.echo_hidden, "echo_hidden should start false");

            app.world_mut().write_message(ConnectionOutput {
                connection: conn_entity,
                text: "".into(),
                echo: Some(false),
                prepend_newline: false,
            });
            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut iac_buf = [0u8; 16];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let n = stream.read(&mut iac_buf).ok().unwrap_or(0);
            assert!(n >= 3, "Should receive IAC WILL ECHO, got {} bytes", n);
            assert_eq!(&iac_buf[..3], &[255, 251, 1], "Should be IAC WILL ECHO");

            let (_, conn2) = query
                .iter(app.world())
                .next()
                .expect("connection should still exist");
            assert!(
                conn2.echo_hidden,
                "echo_hidden should be true after echo: Some(false)"
            );

            drop(stream);
        }

        #[test]
        fn echo_hidden_resets_on_input() {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(TelnetPlugin { port: 19994 });
            add_messages(&mut app);

            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut stream = TcpStream::connect_timeout(
                &"127.0.0.1:19994".parse().unwrap(),
                Duration::from_secs(2),
            )
            .expect("should connect");

            let mut handshake = [0u8; 6];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let _ = stream.read(&mut handshake);

            app.update();

            let mut query = app.world_mut().query::<(Entity, &Connection)>();
            let (conn_entity, _conn) = query
                .iter(app.world())
                .next()
                .expect("should have a connection");

            app.world_mut().write_message(ConnectionOutput {
                connection: conn_entity,
                text: "".into(),
                echo: Some(false),
                prepend_newline: false,
            });
            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut drain = [0u8; 16];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let _ = stream.read(&mut drain); // drain IAC WILL ECHO

            let (_, conn_before) = query.iter(app.world()).next().unwrap();
            assert!(
                conn_before.echo_hidden,
                "echo_hidden should be true before input"
            );

            stream.write_all(b"hello\n").ok();
            std::thread::sleep(Duration::from_millis(50));

            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut response = [0u8; 16];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let n = stream.read(&mut response).ok().unwrap_or(0);
            assert!(n >= 3, "Should receive IAC WONT ECHO, got {} bytes", n);
            assert_eq!(&response[..3], &[255, 252, 1], "Should be IAC WONT ECHO");

            let (_, conn_after) = query.iter(app.world()).next().unwrap();
            assert!(
                !conn_after.echo_hidden,
                "echo_hidden should be false after user input"
            );

            let msg_resource = app.world().resource::<Messages<ConnectionInput>>();
            let mut cursor = msg_resource.get_cursor();
            let events: Vec<&ConnectionInput> = cursor.read(msg_resource).collect();
            let has_hello = events.iter().any(|e| e.text == "hello");
            assert!(has_hello, "ConnectionInput should contain 'hello'");

            drop(stream);
        }
    }

    /// Outbound render: plain text delivery, the in-game `> ` prompt, the
    /// `prepend_newline` leading newline, and the empty-text no-op.
    mod render {
        use super::*;
        use grim_engine_types::components::{Client, ClientState};

        #[test]
        fn send_network_commands_sends_text() {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(TelnetPlugin { port: 19998 });
            add_messages(&mut app);

            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut stream = TcpStream::connect_timeout(
                &"127.0.0.1:19998".parse().unwrap(),
                Duration::from_secs(2),
            )
            .expect("should connect");

            let mut handshake = [0u8; 6];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let _ = stream.read(&mut handshake);

            app.update();

            let mut query = app.world_mut().query::<(Entity, &Connection)>();
            let (conn_entity, _conn) = query
                .iter(app.world())
                .next()
                .expect("should have a connection");

            app.world_mut().write_message(ConnectionOutput {
                connection: conn_entity,
                text: "test message".into(),
                echo: None,
                prepend_newline: false,
            });
            app.update();

            std::thread::sleep(Duration::from_millis(100));

            let mut buf = [0u8; 64];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let n = stream.read(&mut buf).ok().unwrap_or(0);
            let response = String::from_utf8_lossy(&buf[..n]);
            assert!(
                response.contains("test message"),
                "Should receive the sent text"
            );

            drop(stream);
        }

        #[test]
        fn send_network_commands_in_game_prompt() {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(TelnetPlugin { port: 19992 });
            add_messages(&mut app);

            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut stream = TcpStream::connect_timeout(
                &"127.0.0.1:19992".parse().unwrap(),
                Duration::from_secs(2),
            )
            .expect("should connect");

            let mut handshake = [0u8; 6];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let _ = stream.read(&mut handshake);

            app.update();

            let mut query = app.world_mut().query::<(Entity, &Connection)>();
            let (conn_entity, _conn) = query
                .iter(app.world())
                .next()
                .expect("should have a connection");

            let mut client = Client::new(conn_entity);
            client.state = ClientState::InGame;
            app.world_mut().spawn(client);

            app.world_mut().write_message(ConnectionOutput {
                connection: conn_entity,
                text: "Someone enters the room.".into(),
                echo: None,
                prepend_newline: true,
            });
            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut buf = [0u8; 256];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let n = stream.read(&mut buf).ok().unwrap_or(0);
            let response = String::from_utf8_lossy(&buf[..n]);

            assert!(
                response.contains("Someone enters the room."),
                "Response should contain the text, got: {:?}",
                response
            );
            assert!(
                response.contains("> "),
                "In-game response should contain prompt '> ', got: {:?}",
                response
            );

            drop(stream);
        }

        #[test]
        fn send_network_commands_no_prepend_in_game() {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(TelnetPlugin { port: 19991 });
            add_messages(&mut app);

            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut stream = TcpStream::connect_timeout(
                &"127.0.0.1:19991".parse().unwrap(),
                Duration::from_secs(2),
            )
            .expect("should connect");

            let mut handshake = [0u8; 6];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let _ = stream.read(&mut handshake);

            app.update();

            let mut query = app.world_mut().query::<(Entity, &Connection)>();
            let (conn_entity, _conn) = query
                .iter(app.world())
                .next()
                .expect("should have a connection");

            let mut client = Client::new(conn_entity);
            client.state = ClientState::InGame;
            app.world_mut().spawn(client);

            app.world_mut().write_message(ConnectionOutput {
                connection: conn_entity,
                text: "stand".into(),
                echo: None,
                prepend_newline: false,
            });
            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut buf = [0u8; 256];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let n = stream.read(&mut buf).ok().unwrap_or(0);
            let response = String::from_utf8_lossy(&buf[..n]);

            assert!(
                response.contains("stand"),
                "Response should contain 'stand', got: {:?}",
                response
            );
            assert!(
                response.contains("> "),
                "In-game response should contain '> ', got: {:?}",
                response
            );
            assert!(
                !response.starts_with("\r\n"),
                "Response without prepend should not start with \\r\\n, got: {:?}",
                response
            );

            drop(stream);
        }

        #[test]
        fn send_network_commands_empty_text_ingame() {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(TelnetPlugin { port: 19990 });
            add_messages(&mut app);

            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut stream = TcpStream::connect_timeout(
                &"127.0.0.1:19990".parse().unwrap(),
                Duration::from_secs(2),
            )
            .expect("should connect");

            let mut handshake = [0u8; 6];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let _ = stream.read(&mut handshake);

            app.update();

            let mut query = app.world_mut().query::<(Entity, &Connection)>();
            let (conn_entity, _conn) = query
                .iter(app.world())
                .next()
                .expect("should have a connection");

            let mut client = Client::new(conn_entity);
            client.state = ClientState::InGame;
            app.world_mut().spawn(client);

            app.world_mut().write_message(ConnectionOutput {
                connection: conn_entity,
                text: "".into(),
                echo: None,
                prepend_newline: false,
            });
            app.update();
            std::thread::sleep(Duration::from_millis(100));

            let mut buf = [0u8; 8];
            stream
                .set_read_timeout(Some(Duration::from_millis(100)))
                .ok();
            let n = stream.read(&mut buf).ok().unwrap_or(0);
            assert_eq!(n, 0, "No data should be sent for empty text");

            drop(stream);
        }
    }
}
