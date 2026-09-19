//! The tokio↔Bevy bridge: the channel-joined connection registry, the internal
//! event/command types crossing the seam, the per-socket read/write task split
//! (`register_connection`), and the two Bevy systems that drain network events
//! into messages and route outbound messages back to the network.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use grim_core::components::{Client, ClientState};
use grim_networking::{Connection, ConnectionOutput, DisconnectRequest};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::Instant;

use crate::guard::{discard_to_newline, frame_input, rate_tripped, GuardTrip};
use crate::limits::TelnetLimits;
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
        /// Set when an input guard (not a clean EOF) ended the connection.
        reason: Option<GuardTrip>,
    },
    /// The accept loop entered, reported on, or lifted the flood shed.
    /// Heartbeats (`active: true`, `refused > 0`) arrive at most once a
    /// minute while shedding; the exit arrives lazily on the next accept.
    Shed {
        active: bool,
        /// Configured trip threshold (connects per window).
        connects: u32,
        window_secs: u64,
        shed_secs: u64,
        /// Refused connections: 0 on entry, running total on heartbeats
        /// and the exit report.
        refused: u64,
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
///
/// The read task enforces `limits` at framing, before any byte reaches Bevy:
/// lines past `max_line_len` are truncated (remainder discarded to the
/// newline, so a long line never splits into two commands); a line past
/// `max_buffer` without a newline, or more than `max_lines` inside
/// `rate_window_secs`, drops the connection with a [`GuardTrip`] reason.
pub(crate) fn register_connection(
    conn_id: usize,
    socket: TcpStream,
    to_bevy_tx: &std_mpsc::Sender<NetworkEvent>,
    conns: &Arc<Mutex<HashMap<usize, Conn>>>,
    handshake: bool,
    limits: TelnetLimits,
) {
    let raw_fd = socket.as_raw_fd();
    let (read_half, mut write_half) = tokio::io::split(socket);
    let (write_tx, mut write_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(32);

    let read_handle = tokio::spawn({
        let to_bevy_tx = to_bevy_tx.clone();
        let conns = conns.clone();
        async move {
            // A zero cap would read nothing and drop every connection as EOF;
            // floor it instead of trusting configuration.
            let line_cap = limits.max_line_len.max(1);
            let buf_cap = limits.buffer_cap();
            // A zero window would prune every prior timestamp and never trip
            // (fail open); floor it like the line cap.
            let window = std::time::Duration::from_secs(limits.rate_window_secs.max(1));
            let mut line_times: VecDeque<Instant> = VecDeque::new();
            let mut reason: Option<GuardTrip> = None;
            let mut reader = BufReader::new(read_half);
            let mut buf: Vec<u8> = Vec::new();
            loop {
                buf.clear();
                let n = match (&mut reader)
                    .take(line_cap as u64)
                    .read_until(b'\n', &mut buf)
                    .await
                {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                // A short read without a newline is an EOF tail: deliver it
                // exactly as the old uncapped loop did. Only a *full* cap
                // without a newline is an overlong line.
                if n == line_cap && !buf.ends_with(b"\n") {
                    let text = frame_input(&buf);
                    let _ = to_bevy_tx.send(NetworkEvent::Input { conn_id, text });
                    if rate_tripped(&mut line_times, window, limits.max_lines) {
                        reason = Some(GuardTrip::RateExceeded {
                            lines: limits.max_lines,
                            window_secs: limits.rate_window_secs,
                        });
                        break;
                    }
                    // buf doubles as the discard scratch: bounded by take, so
                    // this single call can never run past the budget.
                    if !discard_to_newline(&mut reader, &mut buf, buf_cap - line_cap).await {
                        reason = Some(GuardTrip::BufferExceeded { bytes: buf_cap });
                        break;
                    }
                    continue;
                }
                let text = frame_input(&buf);
                let _ = to_bevy_tx.send(NetworkEvent::Input { conn_id, text });
                if rate_tripped(&mut line_times, window, limits.max_lines) {
                    reason = Some(GuardTrip::RateExceeded {
                        lines: limits.max_lines,
                        window_secs: limits.rate_window_secs,
                    });
                    break;
                }
            }
            let _ = to_bevy_tx.send(NetworkEvent::Disconnected { conn_id, reason });
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
            let (is_ingame, is_afk) = clients
                .iter()
                .find(|c| c.connection == ev.connection)
                .map(|c| (c.state == ClientState::InGame, c.afk))
                .unwrap_or((false, false));
            let ready = render::render_output(&ev.text, is_ingame, is_afk, ev.prepend_newline);
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
    use grim_networking::{
        ConnectionClosed, ConnectionEstablished, ConnectionInput, ConnectionOutput,
        DisconnectRequest, WiznetAlert, WiznetCategory,
    };
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    /// Register the transport messages a bare `App` needs before driving the plugin.
    fn add_messages(app: &mut App) {
        app.add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<DisconnectRequest>()
            .add_message::<WiznetAlert>();
    }

    /// Accept loop: a fresh connection produces `ConnectionEstablished`, input
    /// round-trips to `ConnectionInput`, and a dropped socket yields `ConnectionClosed`.
    mod accept {
        use super::*;

        #[test]
        fn telnet_plugin_accepts_connection() {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(TelnetPlugin::new(19999));
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
            app.add_plugins(TelnetPlugin::new(19993));
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
            app.add_plugins(TelnetPlugin::new(19995));
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
            app.add_plugins(TelnetPlugin::new(19994));
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
        use grim_core::components::{Client, ClientState};

        #[test]
        fn send_network_commands_sends_text() {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(TelnetPlugin::new(19998));
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
            app.add_plugins(TelnetPlugin::new(19992));
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
            app.add_plugins(TelnetPlugin::new(19991));
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
            app.add_plugins(TelnetPlugin::new(19990));
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

    /// Socket-level guard tests on loopback (ports 19996/19997/19989 —
    /// 19990–19995/19998/19999 are taken by the neighboring suites).
    mod guards {
        use super::*;

        fn boot(port: u16, limits: TelnetLimits) -> App {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(TelnetPlugin::new(port).with_limits(limits));
            add_messages(&mut app);
            app.update();
            std::thread::sleep(Duration::from_millis(100));
            app
        }

        fn connect(port: u16) -> std::net::TcpStream {
            let mut stream = std::net::TcpStream::connect_timeout(
                &format!("127.0.0.1:{port}").parse().unwrap(),
                Duration::from_secs(2),
            )
            .expect("should connect");
            // Drain the IAC handshake so later reads see only test bytes.
            let mut handshake = [0u8; 6];
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .ok();
            let _ = stream.read(&mut handshake);
            stream
        }

        fn input_texts(app: &App) -> Vec<String> {
            let msgs = app.world().resource::<Messages<ConnectionInput>>();
            let mut cursor = msgs.get_cursor();
            cursor.read(msgs).map(|e| e.text.clone()).collect()
        }

        fn closed_count(app: &App) -> usize {
            let msgs = app.world().resource::<Messages<ConnectionClosed>>();
            let mut cursor = msgs.get_cursor();
            cursor.read(msgs).count()
        }

        fn small_limits() -> TelnetLimits {
            TelnetLimits {
                max_line_len: 16,
                max_buffer: 64,
                max_lines: 1000,
                rate_window_secs: 60,
                ..TelnetLimits::default()
            }
        }

        #[test]
        fn overlong_line_truncates_without_splitting() {
            let mut app = boot(19996, small_limits());
            let mut stream = connect(19996);
            app.update();
            stream.write_all(&[b'a'; 64]).ok();
            stream.write_all(b"\n").ok();
            stream.write_all(b"ok\n").ok();
            std::thread::sleep(Duration::from_millis(150));
            app.update();
            let texts = input_texts(&app);
            assert_eq!(
                texts,
                vec!["a".repeat(16), "ok".to_string()],
                "truncated prefix then the next line, never a split line"
            );
        }

        #[test]
        fn newline_flood_disconnects() {
            let mut app = boot(19997, small_limits());
            let mut stream = connect(19997);
            app.update();
            // 256 lineless bytes: one truncated fragment is delivered, then
            // the buffer guard drops the connection.
            stream.write_all(&[b'a'; 256]).ok();
            std::thread::sleep(Duration::from_millis(200));
            app.update();
            assert_eq!(
                input_texts(&app),
                vec!["a".repeat(16)],
                "only the truncated fragment escapes"
            );
            assert_eq!(closed_count(&app), 1, "buffer guard must close the socket");
            let mut buf = [0u8; 8];
            stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
            assert_eq!(stream.read(&mut buf).ok(), Some(0), "client must see EOF");
        }

        #[test]
        fn line_burst_disconnects() {
            let limits = TelnetLimits {
                max_lines: 5,
                rate_window_secs: 60,
                ..TelnetLimits::default()
            };
            let mut app = boot(19989, limits);
            let mut stream = connect(19989);
            app.update();
            for i in 0..10 {
                let _ = writeln!(stream, "line{i}");
            }
            std::thread::sleep(Duration::from_millis(200));
            app.update();
            let texts = input_texts(&app);
            assert_eq!(
                texts.len(),
                6,
                "five allowed lines plus the tripping one; got {texts:?}"
            );
            assert_eq!(closed_count(&app), 1, "rate guard must close the socket");
            let mut buf = [0u8; 8];
            stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
            assert_eq!(stream.read(&mut buf).ok(), Some(0), "client must see EOF");
        }

        #[test]
        fn accept_flood_enters_shed_and_drops() {
            let limits = TelnetLimits {
                max_connects_total: 3,
                total_window_secs: 60,
                shed_secs: 3600,
                ..TelnetLimits::default()
            };
            let mut app = boot(19988, limits);
            let mut streams = Vec::new();
            for _ in 0..5 {
                streams.push(connect(19988));
            }
            std::thread::sleep(Duration::from_millis(300));
            app.update();
            // Three admits, the tripping fourth admitted with the entry
            // event, the fifth dropped without a handshake.
            let count = app
                .world_mut()
                .query::<&Connection>()
                .iter(app.world())
                .count();
            assert_eq!(count, 4, "shed must admit four and drop the fifth");
            let mut last = streams.pop().unwrap();
            last.set_read_timeout(Some(Duration::from_secs(2))).ok();
            let mut buf = [0u8; 8];
            assert_eq!(
                last.read(&mut buf).ok(),
                Some(0),
                "shed-dropped socket must see EOF"
            );
        }

        fn alert_texts(app: &App) -> Vec<(WiznetCategory, String)> {
            let msgs = app.world().resource::<Messages<WiznetAlert>>();
            let mut cursor = msgs.get_cursor();
            cursor
                .read(msgs)
                .map(|a| (a.category, a.text.clone()))
                .collect()
        }

        #[test]
        fn connect_and_close_emit_logins_alerts() {
            let mut app = boot(19987, TelnetLimits::default());
            let stream = connect(19987);
            app.update();
            std::thread::sleep(Duration::from_millis(100));
            app.update();
            let alerts = alert_texts(&app);
            assert!(
                alerts.iter().any(|(c, t)| *c == WiznetCategory::Logins
                    && t.starts_with("new connection from 127.0.0.1:")),
                "connect emits a logins alert; got {alerts:?}"
            );
            drop(stream);
            std::thread::sleep(Duration::from_millis(150));
            app.update();
            let alerts = alert_texts(&app);
            assert!(
                alerts.iter().any(|(c, t)| *c == WiznetCategory::Logins
                    && t.ends_with(" closed")
                    && t.contains("127.0.0.1:")),
                "clean close emits a logins alert; got {alerts:?}"
            );
        }

        #[test]
        fn same_tick_connect_and_input_both_land() {
            let mut app = boot(19986, TelnetLimits::default());
            let mut stream = connect(19986);
            // No update between connect and input: both events queue, and
            // one drain pass must deliver both. The spawn from Connected is
            // deferred, so a single-phase drain cannot see it for Input.
            stream.write_all(b"hello\n").ok();
            std::thread::sleep(Duration::from_millis(200));
            app.update();
            let texts = input_texts(&app);
            assert_eq!(
                texts,
                vec!["hello".to_string()],
                "first input after connect must not be dropped"
            );
        }
    }
}
