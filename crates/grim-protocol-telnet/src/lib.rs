use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};

use bevy::log::info;
use bevy::prelude::*;
use grim::prelude::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

// ─── Internal bridge types ─────────────────────────────────────────

struct Conn {
    write_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    read_handle: tokio::task::JoinHandle<()>,
}

enum NetworkEvent {
    Connected { conn_id: usize, addr: SocketAddr },
    Input { conn_id: usize, text: String },
    Disconnected { conn_id: usize },
}

enum NetworkCommand {
    Send { conn_id: usize, text: String },
    SendRaw { conn_id: usize, data: Vec<u8> },
    Disconnect { conn_id: usize },
}

#[derive(Resource)]
struct NetworkBridge {
    to_network: tokio::sync::mpsc::Sender<NetworkCommand>,
    from_network: Arc<Mutex<std_mpsc::Receiver<NetworkEvent>>>,
}

#[derive(Resource)]
struct TelnetPort(pub u16);

// ─── Plugin ─────────────────────────────────────────────────────────

pub struct TelnetPlugin {
    pub port: u16,
}

impl TelnetPlugin {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
}

impl Plugin for TelnetPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ConnectionEstablished>()
            .add_message::<ClientInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ClientOutput>()
            .add_message::<DisconnectRequest>()
            .insert_resource(TelnetPort(self.port))
            .add_systems(Startup, start_telnet_server)
            .add_systems(
                Update,
                (drain_network_events, send_network_commands).chain(),
            );
    }
}

// ─── Startup: spawn the tokio network thread ────────────────────────

fn start_telnet_server(port: Res<TelnetPort>, mut commands: Commands) {
    let port = port.0;

    let (to_bevy_tx, from_network) = std_mpsc::channel::<NetworkEvent>();
    let (to_network, to_tokio_rx) = tokio::sync::mpsc::channel::<NetworkCommand>(64);

    commands.insert_resource(NetworkBridge {
        to_network,
        from_network: Arc::new(Mutex::new(from_network)),
    });

    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                let listener = TcpListener::bind(("0.0.0.0", port))
                    .await
                    .expect("failed to bind telnet listener");
                info!("Telnet server listening on port {}", port);
                let next_id = AtomicUsize::new(1);
                let conns: Arc<Mutex<HashMap<usize, Conn>>> = Arc::new(Mutex::new(HashMap::new()));
                let mut to_tokio_rx = to_tokio_rx;

                loop {
                    tokio::select! {
                        Ok((socket, addr)) = listener.accept() => {
                            let conn_id = next_id.fetch_add(1, Ordering::Relaxed);
                            let _ = to_bevy_tx.send(NetworkEvent::Connected { conn_id, addr });

                            // Minimal telnet handshake: IAC WILL ECHO, IAC WILL SUPPRESS_GO_AHEAD.
                            let (read_half, mut write_half) = tokio::io::split(socket);
                            let _ = write_half.write_all(&[255, 253, 1, 255, 253, 3]).await;

                            let (write_tx, write_rx) =
                                tokio::sync::mpsc::channel::<Vec<u8>>(32);

                            // Read task — handle stored to abort on clean disconnect.
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

                                        // Strip telnet IAC sequences (0xFF cmd1 cmd2).
                                        let mut clean = Vec::with_capacity(buf.len());
                                        let mut i = 0;
                                        while i < buf.len() {
                                            if buf[i] == 0xFF && i + 2 < buf.len() {
                                                i += 3;
                                                continue;
                                            }
                                            clean.push(buf[i]);
                                            i += 1;
                                        }

                                        let text = String::from_utf8_lossy(&clean)
                                            .trim_end_matches(['\r', '\n'])
                                            .to_string();
                                        let _ = to_bevy_tx.send(NetworkEvent::Input {
                                            conn_id, text,
                                        });
                                    }
                                    let _ = to_bevy_tx.send(NetworkEvent::Disconnected { conn_id });
                                    conns.lock().unwrap().remove(&conn_id);
                                }
                            });

                            conns.lock().unwrap().insert(conn_id, Conn { write_tx, read_handle });

                            // Write task: drain per-connection channel -> socket.
                            tokio::spawn(async move {
                                let mut write_rx = write_rx;
                                while let Some(data) = write_rx.recv().await {
                                    if write_half.write_all(&data).await.is_err() {
                                        break;
                                    }
                                }
                            });
                        }
                        Some(cmd) = to_tokio_rx.recv() => {
                            match cmd {
                                NetworkCommand::Send { conn_id, text } => {
                                    let guard = conns.lock().unwrap();
                                    if let Some(tx) = guard.get(&conn_id) {
                                        let _ = tx.write_tx.try_send(text.into_bytes());
                                    }
                                }
                                NetworkCommand::SendRaw { conn_id, data } => {
                                    let guard = conns.lock().unwrap();
                                    if let Some(tx) = guard.get(&conn_id) {
                                        let _ = tx.write_tx.try_send(data);
                                    }
                                }
                                NetworkCommand::Disconnect { conn_id } => {
                                    if let Some(conn) = conns.lock().unwrap().remove(&conn_id) {
                                        conn.read_handle.abort();
                                    }
                                }
                            }
                        }
                    }
                }
            });
    });
}

// ─── Update: drain network -> Bevy events ──────────────────────────

fn drain_network_events(
    bridge: Res<NetworkBridge>,
    mut commands: Commands,
    mut established: MessageWriter<ConnectionEstablished>,
    mut input: MessageWriter<ClientInput>,
    mut closed: MessageWriter<ConnectionClosed>,
    connections: Query<(Entity, &Connection)>,
) {
    let rx = bridge.from_network.lock().unwrap();
    while let Ok(ev) = rx.try_recv() {
        match ev {
            NetworkEvent::Connected { conn_id, addr } => {
                info!("Connection from {} (id={})", addr, conn_id);
                let entity = commands.spawn(Connection { id: conn_id, addr }).id();
                established.write(ConnectionEstablished {
                    connection: entity,
                    addr,
                });
            }
            NetworkEvent::Input { conn_id, text } => {
                if let Some((entity, _)) = connections.iter().find(|(_, c)| c.id == conn_id) {
                    input.write(ClientInput {
                        connection: entity,
                        text,
                    });
                }
            }
            NetworkEvent::Disconnected { conn_id } => {
                info!("Connection {} disconnected", conn_id);
                if let Some((entity, _)) = connections.iter().find(|(_, c)| c.id == conn_id) {
                    closed.write(ConnectionClosed { connection: entity });
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

// ─── Update: route Bevy events -> network ──────────────────────────

fn send_network_commands(
    bridge: Res<NetworkBridge>,
    mut output: MessageReader<ClientOutput>,
    mut disconnect: MessageReader<DisconnectRequest>,
    connections: Query<&Connection>,
) {
    for ev in output.read() {
        if let Ok(conn) = connections.get(ev.connection) {
            if let Some(echo_state) = ev.echo {
                let data = if echo_state {
                    vec![255, 252, 1] // IAC WONT ECHO → visible input (normal)
                } else {
                    vec![255, 251, 1] // IAC WILL ECHO → hidden input (password)
                };
                let _ = bridge.to_network.try_send(NetworkCommand::SendRaw {
                    conn_id: conn.id,
                    data,
                });
            }
            let _ = bridge.to_network.try_send(NetworkCommand::Send {
                conn_id: conn.id,
                text: ev.text.clone(),
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
