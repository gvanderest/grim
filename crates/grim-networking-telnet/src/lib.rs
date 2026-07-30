use grim_color::ansi;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};

use bevy::log::{error, info, warn};
use bevy::prelude::*;
use grim_engine_types::components::{Character, Client, ClientState, Linkdead};
use grim_networking::{
    Connection, ConnectionClosed, ConnectionEstablished, ConnectionInput, ConnectionOutput,
    ConnectionResumed, DisconnectRequest, HandoverEntry, HandoverManifest,
};
use sendfd::{RecvWithFd, SendWithFd};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// Env var carrying the unix-socket path a copyover successor uses to receive
/// the live listener + client sockets from its predecessor. Its presence on
/// startup means "you are the successor: adopt fds instead of binding fresh".
const COPYOVER_SOCK_ENV: &str = "GRIM_COPYOVER_SOCK";

// ─── Internal bridge types ─────────────────────────────────────────

struct Conn {
    write_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    #[allow(dead_code)]
    read_handle: tokio::task::JoinHandle<()>,
    /// Raw fd of the underlying socket, recorded at accept/adopt time. Stays
    /// valid while the socket lives; sent (dup'd) to the successor on copyover.
    raw_fd: RawFd,
}

enum NetworkEvent {
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

enum NetworkCommand {
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
struct CopyoverConn {
    conn_id: usize,
    character: String,
    echo_hidden: bool,
}

#[derive(Resource)]
struct NetworkBridge {
    to_network: tokio::sync::mpsc::Sender<NetworkCommand>,
    from_network: Arc<Mutex<std_mpsc::Receiver<NetworkEvent>>>,
}

#[derive(Resource)]
struct TelnetPort(pub u16);

/// Set by the `SIGUSR2` handler; drained by `poll_copyover_signal` on the next
/// tick (a signal handler can safely do little more than flip a flag).
#[derive(Resource, Clone)]
struct CopyoverSignal(Arc<AtomicBool>);

impl Default for CopyoverSignal {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}

/// Flipped by the tokio thread once the successor has acknowledged the handoff;
/// `finish_copyover` then exits this (predecessor) process cleanly.
#[derive(Resource, Clone)]
struct CopyoverDone(Arc<AtomicBool>);

impl Default for CopyoverDone {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}

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
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<ConnectionResumed>()
            .add_message::<DisconnectRequest>()
            .insert_resource(TelnetPort(self.port))
            .init_resource::<CopyoverSignal>()
            .init_resource::<CopyoverDone>()
            .add_systems(Startup, (install_copyover_signal, start_telnet_server))
            .add_systems(
                Update,
                (
                    drain_network_events,
                    send_network_commands,
                    poll_copyover_signal,
                    finish_copyover,
                )
                    .chain(),
            );
    }
}

/// Wire `SIGUSR2` to the copyover flag. Failure is logged, not fatal.
fn install_copyover_signal(signal: Res<CopyoverSignal>) {
    match signal_hook::flag::register(signal_hook::consts::SIGUSR2, signal.0.clone()) {
        Ok(_) => info!("SIGUSR2 will trigger a copyover (hot restart)"),
        Err(e) => warn!("failed to register SIGUSR2 handler: {e}"),
    }
}

// ─── Startup: spawn the tokio network thread ────────────────────────

fn start_telnet_server(port: Res<TelnetPort>, done: Res<CopyoverDone>, mut commands: Commands) {
    let port = port.0;

    let (to_bevy_tx, from_network) = std_mpsc::channel::<NetworkEvent>();
    let (to_network, to_tokio_rx) = tokio::sync::mpsc::channel::<NetworkCommand>(64);

    commands.insert_resource(NetworkBridge {
        to_network,
        from_network: Arc::new(Mutex::new(from_network)),
    });

    let copyover_done = done.0.clone();

    std::thread::spawn(move || {
        // Receive any handoff *before* building the runtime — blocking std I/O,
        // and the raw fds must be adopted before tokio touches them.
        let handoff = std::env::var(COPYOVER_SOCK_ENV)
            .ok()
            .map(|path| receive_handoff(&path));

        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                let mut to_tokio_rx = to_tokio_rx;
                let next_id = AtomicUsize::new(1);
                let conns: Arc<Mutex<HashMap<usize, Conn>>> = Arc::new(Mutex::new(HashMap::new()));

                // Adopt the inherited listener + sockets, or bind fresh.
                let (listener, resumed, ack) = match handoff {
                    Some(Ok(h)) => {
                        let listener = match TcpListener::from_std(h.listener) {
                            Ok(l) => l,
                            Err(e) => {
                                // Can't serve without the inherited listener — a
                                // detached network thread quietly dying would leave
                                // a listener-less zombie MUD, so fail the process.
                                error!("failed to adopt inherited listener: {e}");
                                std::process::exit(1);
                            }
                        };
                        info!(
                            "Telnet server resumed via copyover with {} connection(s)",
                            h.conns.len()
                        );
                        (listener, h.conns, Some(h.ack))
                    }
                    Some(Err(e)) => {
                        error!("copyover receive failed: {e}; binding fresh on {port}");
                        (bind_fresh(port).await, Vec::new(), None)
                    }
                    None => (bind_fresh(port).await, Vec::new(), None),
                };
                let listener_fd = listener.as_raw_fd();

                // Re-adopt each carried socket into a live connection and ask the
                // session layer to resume its character (no login, no banner).
                for (fd, entry) in resumed {
                    // SAFETY: `fd` was just delivered to us via SCM_RIGHTS and is
                    // not owned by anything else in this process.
                    let std_stream = unsafe { std::net::TcpStream::from_raw_fd(fd) };
                    if std_stream.set_nonblocking(true).is_err() {
                        continue;
                    }
                    let addr = std_stream
                        .peer_addr()
                        .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());
                    let Ok(socket) = TcpStream::from_std(std_stream) else {
                        continue;
                    };
                    let conn_id = next_id.fetch_add(1, Ordering::Relaxed);
                    register_connection(conn_id, socket, &to_bevy_tx, &conns, false);
                    let _ = to_bevy_tx.send(NetworkEvent::Resumed {
                        conn_id,
                        addr,
                        character: entry.character,
                        echo_hidden: entry.echo_hidden,
                    });
                }

                // Always signal readiness: the systemd unit is `Type=notify`, so
                // even a fresh start must send READY=1 or the service is
                // considered to have timed out. On a copyover the accompanying
                // MAINPID reassigns supervision to us *before* the predecessor
                // exits, so its exit doesn't tear the service down. Both no-op
                // outside systemd (NOTIFY_SOCKET unset).
                let _ = sd_notify::notify(&[
                    sd_notify::NotifyState::MainPid(std::process::id()),
                    sd_notify::NotifyState::Ready,
                ]);
                // Acknowledge the predecessor (copyover only) so it can exit now
                // that we are serving and have claimed MAINPID.
                if let Some(mut ack) = ack {
                    use std::io::Write;
                    let _ = ack.write_all(&[1u8]);
                }

                // `accepting` gates new connections; it is turned off for the
                // brief window of a copyover handoff.
                let mut accepting = true;

                loop {
                    tokio::select! {
                        Ok((socket, addr)) = listener.accept(), if accepting => {
                            let conn_id = next_id.fetch_add(1, Ordering::Relaxed);
                            let _ = to_bevy_tx.send(NetworkEvent::Connected { conn_id, addr });
                            register_connection(conn_id, socket, &to_bevy_tx, &conns, true);
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
                                NetworkCommand::Copyover { conns: list } => {
                                    accepting = false;
                                    // Join each requested connection with its raw fd
                                    // and build the ordered manifest (listener fd
                                    // first, then one fd per manifest entry).
                                    let mut fds = vec![listener_fd];
                                    let mut entries = Vec::new();
                                    {
                                        let guard = conns.lock().unwrap();
                                        for c in &list {
                                            if let Some(conn) = guard.get(&c.conn_id) {
                                                fds.push(conn.raw_fd);
                                                entries.push(HandoverEntry {
                                                    character: c.character.clone(),
                                                    echo_hidden: c.echo_hidden,
                                                });
                                                let _ = conn.write_tx.try_send(
                                                    b"\r\n[SERVER] Reloading, hold on...\r\n".to_vec(),
                                                );
                                            }
                                        }
                                    }
                                    // Let the notices flush to the sockets before
                                    // we hand the fds to the successor.
                                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                    let manifest = HandoverManifest { entries };
                                    let done = copyover_done.clone();
                                    match tokio::task::spawn_blocking(move || {
                                        perform_handoff(&manifest, &fds)
                                    })
                                    .await
                                    {
                                        Ok(Ok(())) => {
                                            info!("copyover handoff complete; exiting");
                                            done.store(true, Ordering::SeqCst);
                                        }
                                        Ok(Err(e)) => {
                                            error!("copyover handoff failed: {e}; resuming service");
                                            accepting = true;
                                        }
                                        Err(e) => {
                                            error!("copyover task panicked: {e}; resuming service");
                                            accepting = true;
                                        }
                                    }
                                }
                            }
                        }
                        // All senders dropped (the app is exiting, e.g. right
                        // after a copyover handoff) and we've stopped accepting —
                        // end the loop cleanly instead of panicking on an
                        // all-branches-disabled select.
                        else => break,
                    }
                }
            });
    });
}

/// Bind the telnet listener fresh (the non-copyover path).
async fn bind_fresh(port: u16) -> TcpListener {
    match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => {
            info!("Telnet server listening on port {}", port);
            listener
        }
        Err(e) => {
            // The listener lives on a detached thread, so a panic here would be
            // swallowed and leave the MUD running with no transport. Exit the
            // whole process instead (systemd `Restart=on-failure` handles the
            // real box; locally you get a clean, obvious crash).
            error!("failed to bind telnet listener on port {port}: {e}");
            std::process::exit(1);
        }
    }
}

/// Split `socket` into read/write tasks and register it under `conn_id`. Fresh
/// accepts send the minimal telnet negotiation first (`handshake`); re-adopted
/// copyover sockets have already negotiated, so they skip it.
fn register_connection(
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
                let _ = to_bevy_tx.send(NetworkEvent::Input { conn_id, text });
            }
            let _ = to_bevy_tx.send(NetworkEvent::Disconnected { conn_id });
            conns.lock().unwrap().remove(&conn_id);
        }
    });

    conns.lock().unwrap().insert(
        conn_id,
        Conn {
            write_tx,
            read_handle,
            raw_fd,
        },
    );

    tokio::spawn(async move {
        if handshake {
            // IAC WILL ECHO, IAC WILL SUPPRESS_GO_AHEAD.
            let _ = write_half.write_all(&[255, 253, 1, 255, 253, 3]).await;
        }
        while let Some(data) = write_rx.recv().await {
            if write_half.write_all(&data).await.is_err() {
                break;
            }
        }
    });
}

/// Live sockets received from a predecessor during a copyover.
struct Handoff {
    /// The inherited listener, already set non-blocking.
    listener: std::net::TcpListener,
    /// Client sockets paired with the character to resume on each.
    conns: Vec<(RawFd, HandoverEntry)>,
    /// The handoff channel, kept open so we can acknowledge once we're serving.
    ack: UnixStream,
}

fn invalid_data(e: impl std::error::Error + Send + Sync + 'static) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
}

/// Path to exec for the copyover successor. Copyover exists to swap *to a new
/// binary*, so by the time it runs the old binary file has been replaced (the
/// deploy `mv`s it; a local `cargo build` relinks it). On Linux that unlinks the
/// running image's inode, and `current_exe()` then reports the path with a
/// trailing " (deleted)" — which does not exist, so spawning it fails. Strip that
/// marker to get the path now holding the *new* binary, which is exactly what we
/// want to exec.
fn current_exe_path() -> std::io::Result<std::path::PathBuf> {
    let exe = std::env::current_exe()?;
    if let Some(s) = exe.to_str() {
        if let Some(stripped) = s.strip_suffix(" (deleted)") {
            return Ok(std::path::PathBuf::from(stripped));
        }
    }
    Ok(exe)
}

/// Serialize `manifest` and send it together with `fds` (listener first) over a
/// unix socket via SCM_RIGHTS. The framing half of the predecessor handoff, split
/// out so it can be unit-tested without spawning a process.
fn write_handoff(
    stream: &UnixStream,
    manifest: &HandoverManifest,
    fds: &[RawFd],
) -> std::io::Result<()> {
    let json = serde_json::to_vec(manifest).map_err(invalid_data)?;
    stream.send_with_fd(&json, fds)?;
    Ok(())
}

/// Receive the manifest + fds sent by [`write_handoff`]. Returns the manifest,
/// the listener fd (`fds[0]`), and the client fds (`fds[1..]`, aligned with
/// `manifest.entries`). The framing half of the successor handoff.
fn read_handoff(stream: &UnixStream) -> std::io::Result<(HandoverManifest, RawFd, Vec<RawFd>)> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut fds = [0 as RawFd; 512];
    let (n, fd_count) = stream.recv_with_fd(&mut buf, &mut fds)?;
    if fd_count == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "copyover handoff carried no listener fd",
        ));
    }
    let manifest: HandoverManifest = serde_json::from_slice(&buf[..n]).map_err(invalid_data)?;
    let listener_fd = fds[0];
    let conn_fds = fds[1..fd_count].to_vec();
    Ok((manifest, listener_fd, conn_fds))
}

/// Predecessor side of a copyover: spawn the successor, hand it the listener +
/// client fds and the manifest over a unix socket, and wait for its ack.
fn perform_handoff(manifest: &HandoverManifest, fds: &[RawFd]) -> std::io::Result<()> {
    use std::io::Read;
    let sock_path = std::env::temp_dir().join(format!("grim-copyover-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path)?;

    let exe = current_exe_path()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.env(COPYOVER_SOCK_ENV, &sock_path);
    let _child = cmd.spawn()?;

    let (mut stream, _addr) = listener.accept()?;
    write_handoff(&stream, manifest, fds)?;

    // Wait for the successor to confirm it is serving before we let the process
    // exit — this keeps the fds (and the systemd MainPID handoff) valid until
    // the new instance has taken over.
    let mut ack = [0u8; 1];
    stream.read_exact(&mut ack)?;
    let _ = std::fs::remove_file(&sock_path);
    Ok(())
}

/// Successor side of a copyover: connect to the predecessor and receive the
/// listener + client fds plus the manifest. Blocking; run before the runtime.
fn receive_handoff(path: &str) -> std::io::Result<Handoff> {
    let stream = UnixStream::connect(path)?;
    let (manifest, listener_fd, conn_fds) = read_handoff(&stream)?;

    // SAFETY: each fd was just delivered via SCM_RIGHTS and is owned by no one
    // else in this process.
    let listener = unsafe { std::net::TcpListener::from_raw_fd(listener_fd) };
    listener.set_nonblocking(true)?;

    // fds[1..] pair with manifest entries in order; any entry without a matching
    // fd is dropped rather than mis-adopted.
    let conns = manifest
        .entries
        .into_iter()
        .enumerate()
        .filter_map(|(i, entry)| conn_fds.get(i).map(|&fd| (fd, entry)))
        .collect();

    Ok(Handoff {
        listener,
        conns,
        ack: stream,
    })
}

// ─── Update: drain network -> Bevy events ──────────────────────────

fn drain_network_events(
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
                            data: vec![255, 252, 1], // IAC WONT ECHO → visible
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

fn send_network_commands(
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
                    vec![255, 252, 1] // IAC WONT ECHO → visible input (normal)
                } else {
                    vec![255, 251, 1] // IAC WILL ECHO → hidden input (password)
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

            // Prepend a newline for unsolicited events so they don't appear on the prompt line.
            let mut text = ev.text.clone();
            if ev.prepend_newline && !text.is_empty() {
                text.insert(0, '\n');
            }
            let send_text = if is_ingame && !text.is_empty() {
                format!("{}\n> ", text)
            } else {
                text
            };
            let palette = grim_color::convert_16color(&send_text);
            let colored = ansi(&palette);
            let ready = colored.replace('\n', "\r\n");
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

// ─── Copyover ───────────────────────────────────────────────────────

/// If `SIGUSR2` fired, snapshot the in-game connections and ask the tokio thread
/// to hand them to a successor process. Runs once — `started` latches so a
/// second signal mid-handoff is ignored.
fn poll_copyover_signal(
    signal: Res<CopyoverSignal>,
    bridge: Res<NetworkBridge>,
    clients: Query<&Client>,
    characters: Query<&Character>,
    linkdead: Query<&Linkdead>,
    connections: Query<&Connection>,
    mut started: Local<bool>,
) {
    if !signal.0.swap(false, Ordering::SeqCst) {
        return;
    }
    if *started {
        return;
    }
    // Only actively-playing sessions carry across: in-game state, a bound
    // character, and not linkdead. Anyone at the login prompt or linkdead is
    // dropped and reconnects fresh.
    let mut list = Vec::new();
    for client in clients.iter() {
        if client.state != ClientState::InGame {
            continue;
        }
        let Some(char_entity) = client.character else {
            continue;
        };
        if linkdead.get(char_entity).is_ok() {
            continue;
        }
        let Ok(character) = characters.get(char_entity) else {
            continue;
        };
        let Ok(conn) = connections.get(client.connection) else {
            continue;
        };
        list.push(CopyoverConn {
            conn_id: conn.id,
            character: character.name.clone(),
            echo_hidden: conn.echo_hidden,
        });
    }
    info!(
        "copyover requested: handing off {} connection(s)",
        list.len()
    );
    let _ = bridge
        .to_network
        .try_send(NetworkCommand::Copyover { conns: list });
    *started = true;
}

/// Once the tokio thread reports the successor has taken over, exit cleanly so
/// the predecessor process goes away.
fn finish_copyover(done: Res<CopyoverDone>, mut exit: MessageWriter<AppExit>) {
    if done.0.load(Ordering::SeqCst) {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    /// The copyover framing round-trips the manifest and real fds over a unix
    /// socket in-process (no child spawn). Covers the `write_handoff`/`read_handoff`
    /// SCM_RIGHTS + serde path that the process-level integration test can't
    /// reliably measure (its successor is SIGKILLed).
    #[test]
    fn handoff_round_trips_manifest_and_fds() {
        use std::os::unix::net::UnixStream as StdUnix;

        let (tx, rx) = StdUnix::pair().unwrap();
        // Two throwaway sockets whose fds stand in for "listener" + one client.
        let (a, _a2) = StdUnix::pair().unwrap();
        let (b, _b2) = StdUnix::pair().unwrap();

        let manifest = HandoverManifest {
            entries: vec![HandoverEntry {
                character: "Alice".into(),
                echo_hidden: false,
            }],
        };
        write_handoff(&tx, &manifest, &[a.as_raw_fd(), b.as_raw_fd()]).unwrap();

        let (got, listener_fd, conn_fds) = read_handoff(&rx).unwrap();
        assert_eq!(got.entries.len(), 1);
        assert_eq!(got.entries[0].character, "Alice");
        assert!(listener_fd >= 0, "listener fd delivered");
        assert_eq!(conn_fds.len(), 1, "one client fd, aligned with the entry");
        assert!(conn_fds[0] >= 0);

        // The received fds are dups owned by us now — wrap + drop to close them.
        let _l = unsafe { StdUnix::from_raw_fd(listener_fd) };
        let _c = unsafe { StdUnix::from_raw_fd(conn_fds[0]) };
    }

    /// A handoff carrying no fds is rejected rather than silently adopting a bad
    /// listener.
    #[test]
    fn read_handoff_rejects_empty_fd_set() {
        use std::os::unix::net::UnixStream as StdUnix;
        let (tx, rx) = StdUnix::pair().unwrap();
        write_handoff(&tx, &HandoverManifest::default(), &[]).unwrap();
        assert!(read_handoff(&rx).is_err(), "no listener fd → error");
    }

    #[test]
    fn test_telnet_plugin_accepts_connection() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(TelnetPlugin { port: 19999 });
        app.add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<DisconnectRequest>();

        app.update();

        std::thread::sleep(Duration::from_millis(100));

        let mut stream =
            TcpStream::connect_timeout(&"127.0.0.1:19999".parse().unwrap(), Duration::from_secs(2))
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
    fn test_send_network_commands_sends_text() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(TelnetPlugin { port: 19998 });
        app.add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<DisconnectRequest>();

        app.update();
        std::thread::sleep(Duration::from_millis(100));

        let mut stream =
            TcpStream::connect_timeout(&"127.0.0.1:19998".parse().unwrap(), Duration::from_secs(2))
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
    fn test_telnet_plugin_new() {
        // L50-52: TelnetPlugin::new() — just instantiate it
        let plugin = TelnetPlugin::new(8080);
        assert_eq!(plugin.port, 8080);

        let plugin2 = TelnetPlugin { port: 9090 };
        assert_eq!(plugin2.port, 9090);
    }

    #[test]
    fn test_echo_false_sets_echo_hidden() {
        // L166-177: send_network_commands with echo: Some(false) sends IAC WILL ECHO
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(TelnetPlugin { port: 19995 });
        app.add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<DisconnectRequest>();

        app.update();
        std::thread::sleep(Duration::from_millis(100));

        let mut stream =
            TcpStream::connect_timeout(&"127.0.0.1:19995".parse().unwrap(), Duration::from_secs(2))
                .expect("should connect");

        // Drain telnet handshake
        let mut handshake = [0u8; 6];
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .ok();
        let _ = stream.read(&mut handshake);

        app.update();

        // Find the connection entity
        let mut query = app.world_mut().query::<(Entity, &Connection)>();
        let (conn_entity, conn) = query
            .iter(app.world())
            .next()
            .expect("should have a connection");
        assert!(!conn.echo_hidden, "echo_hidden should start false");

        // Send ConnectionOutput with echo: Some(false) → sends IAC WILL ECHO, sets echo_hidden=true
        app.world_mut().write_message(ConnectionOutput {
            connection: conn_entity,
            text: "".into(),
            echo: Some(false),
            prepend_newline: false,
        });
        app.update();
        std::thread::sleep(Duration::from_millis(100));

        // Read IAC WILL ECHO from TCP
        let mut iac_buf = [0u8; 16];
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .ok();
        let n = stream.read(&mut iac_buf).ok().unwrap_or(0);
        assert!(n >= 3, "Should receive IAC WILL ECHO, got {} bytes", n);
        assert_eq!(&iac_buf[..3], &[255, 251, 1], "Should be IAC WILL ECHO");

        // Verify connection state
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
    fn test_echo_hidden_resets_on_input() {
        // L225-231: drain_network_events with echo_hidden=true triggers IAC WONT ECHO
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(TelnetPlugin { port: 19994 });
        app.add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<DisconnectRequest>();

        app.update();
        std::thread::sleep(Duration::from_millis(100));

        let mut stream =
            TcpStream::connect_timeout(&"127.0.0.1:19994".parse().unwrap(), Duration::from_secs(2))
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

        // Set echo_hidden=true via echo: Some(false)
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

        // Verify echo_hidden is true
        let (_, conn_before) = query.iter(app.world()).next().unwrap();
        assert!(
            conn_before.echo_hidden,
            "echo_hidden should be true before input"
        );

        // Send user input from the TCP client
        stream.write_all(b"hello\n").ok();
        std::thread::sleep(Duration::from_millis(50));

        app.update();
        std::thread::sleep(Duration::from_millis(100));

        // Read IAC WONT ECHO from TCP
        let mut response = [0u8; 16];
        stream
            .set_read_timeout(Some(Duration::from_millis(200)))
            .ok();
        let n = stream.read(&mut response).ok().unwrap_or(0);
        assert!(n >= 3, "Should receive IAC WONT ECHO, got {} bytes", n);
        assert_eq!(&response[..3], &[255, 252, 1], "Should be IAC WONT ECHO");

        // Verify echo_hidden was reset to false
        let (_, conn_after) = query.iter(app.world()).next().unwrap();
        assert!(
            !conn_after.echo_hidden,
            "echo_hidden should be false after user input"
        );

        // ConnectionInput should have been written
        let msg_resource = app.world().resource::<Messages<ConnectionInput>>();
        let mut cursor = msg_resource.get_cursor();
        let events: Vec<&ConnectionInput> = cursor.read(msg_resource).collect();
        let has_hello = events.iter().any(|e| e.text == "hello");
        assert!(has_hello, "ConnectionInput should contain 'hello'");

        drop(stream);
    }

    #[test]
    fn test_input_filter_strips_control_chars() {
        // L221: Input filter keeps only ASCII 32-126
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(TelnetPlugin { port: 19993 });
        app.add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<DisconnectRequest>();

        app.update();
        std::thread::sleep(Duration::from_millis(100));

        let mut stream =
            TcpStream::connect_timeout(&"127.0.0.1:19993".parse().unwrap(), Duration::from_secs(2))
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

        // Verify control chars are stripped
        let msg_resource = app.world().resource::<Messages<ConnectionInput>>();
        let mut cursor = msg_resource.get_cursor();
        let events: Vec<&ConnectionInput> = cursor.read(msg_resource).collect();
        let has_filtered = events.iter().any(|e| e.text == "helloworld");
        assert!(
            has_filtered,
            "Control chars should be stripped, got: {:?}",
            events.iter().map(|e| &e.text).collect::<Vec<_>>()
        );

        // Verify tab is stripped (not a space) — tab is not in ASCII 32-126 range
        let has_raw = events.iter().any(|e| e.text.contains('\t'));
        assert!(!has_raw, "Tab should be stripped");

        drop(stream);
    }

    #[test]
    fn test_send_network_commands_in_game_prompt() {
        // L282-295: In-game client gets "> " prompt appended
        // L287-290: prepend_newline=true adds \n before text
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(TelnetPlugin { port: 19992 });
        app.add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<DisconnectRequest>();

        app.update();
        std::thread::sleep(Duration::from_millis(100));

        let mut stream =
            TcpStream::connect_timeout(&"127.0.0.1:19992".parse().unwrap(), Duration::from_secs(2))
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

        // Spawn an in-game Client linked to this connection
        use grim_engine_types::components::{Client, ClientState};
        let mut client = Client::new(conn_entity);
        client.state = ClientState::InGame;
        app.world_mut().spawn(client);

        // Send output with prepend_newline=true
        app.world_mut().write_message(ConnectionOutput {
            connection: conn_entity,
            text: "Someone enters the room.".into(),
            echo: None,
            prepend_newline: true,
        });
        app.update();
        std::thread::sleep(Duration::from_millis(100));

        // Read response — should contain the text with prompt
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
    fn test_send_network_commands_no_prepend_in_game() {
        // L287-295: prepend_newline=false for in-game; text with prompt but no leading \n
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(TelnetPlugin { port: 19991 });
        app.add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<DisconnectRequest>();

        app.update();
        std::thread::sleep(Duration::from_millis(100));

        let mut stream =
            TcpStream::connect_timeout(&"127.0.0.1:19991".parse().unwrap(), Duration::from_secs(2))
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

        use grim_engine_types::components::{Client, ClientState};
        let mut client = Client::new(conn_entity);
        client.state = ClientState::InGame;
        app.world_mut().spawn(client);

        // Send output with prepend_newline=false
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

        // Should contain "stand" and prompt, but NOT start with \r\n
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
    fn test_send_network_commands_empty_text_ingame() {
        // L291-294: Empty text with is_ingame — goes through else branch (empty string)
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(TelnetPlugin { port: 19990 });
        app.add_message::<ConnectionEstablished>()
            .add_message::<ConnectionInput>()
            .add_message::<ConnectionClosed>()
            .add_message::<ConnectionOutput>()
            .add_message::<DisconnectRequest>();

        app.update();
        std::thread::sleep(Duration::from_millis(100));

        let mut stream =
            TcpStream::connect_timeout(&"127.0.0.1:19990".parse().unwrap(), Duration::from_secs(2))
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

        use grim_engine_types::components::{Client, ClientState};
        let mut client = Client::new(conn_entity);
        client.state = ClientState::InGame;
        app.world_mut().spawn(client);

        // Empty text with in-game client — goes through else branch
        app.world_mut().write_message(ConnectionOutput {
            connection: conn_entity,
            text: "".into(),
            echo: None,
            prepend_newline: false,
        });
        app.update();
        std::thread::sleep(Duration::from_millis(100));

        // Nothing should be sent for empty text
        let mut buf = [0u8; 8];
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .ok();
        let n = stream.read(&mut buf).ok().unwrap_or(0);
        assert_eq!(n, 0, "No data should be sent for empty text");

        drop(stream);
    }
}
