//! Copyover (hot restart): swap the server binary without disconnecting players.
//! `SIGUSR2` flips a flag; the predecessor hands its live listener + in-game
//! client sockets to a freshly-spawned successor over a unix socket
//! (`SCM_RIGHTS`, via the `sendfd` crate), waits for the successor to ack, then
//! exits. This module owns the fd framing, the process-level handoff, the signal
//! plumbing, and the two Bevy systems that snapshot sessions and finish the exit.

use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bevy::log::{error, info, warn};
use bevy::prelude::*;
use grim_engine_types::components::{Character, Client, ClientState, Linkdead};
use grim_networking::{Connection, HandoverEntry, HandoverManifest};
use sendfd::{RecvWithFd, SendWithFd};

use crate::bridge::{CopyoverConn, NetworkBridge, NetworkCommand};

/// Env var carrying the unix-socket path a copyover successor uses to receive
/// the live listener + client sockets from its predecessor. Its presence on
/// startup means "you are the successor: adopt fds instead of binding fresh".
pub(crate) const COPYOVER_SOCK_ENV: &str = "GRIM_COPYOVER_SOCK";

/// Set by the `SIGUSR2` handler; drained by `poll_copyover_signal` on the next
/// tick (a signal handler can safely do little more than flip a flag).
#[derive(Resource, Clone)]
pub(crate) struct CopyoverSignal(pub(crate) Arc<AtomicBool>);

impl Default for CopyoverSignal {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}

/// Flipped by the tokio thread once the successor has acknowledged the handoff;
/// `finish_copyover` then exits this (predecessor) process cleanly.
#[derive(Resource, Clone)]
pub(crate) struct CopyoverDone(pub(crate) Arc<AtomicBool>);

impl Default for CopyoverDone {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}

/// Live sockets received from a predecessor during a copyover.
pub(crate) struct Handoff {
    /// The inherited listener, already set non-blocking.
    pub(crate) listener: std::net::TcpListener,
    /// Client sockets paired with the character to resume on each.
    pub(crate) conns: Vec<(RawFd, HandoverEntry)>,
    /// The handoff channel, kept open so we can acknowledge once we're serving.
    pub(crate) ack: UnixStream,
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
pub(crate) fn write_handoff(
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
pub(crate) fn read_handoff(
    stream: &UnixStream,
) -> std::io::Result<(HandoverManifest, RawFd, Vec<RawFd>)> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut fds = [0 as RawFd; 512];
    let (n, fd_count) = stream.recv_with_fd(&mut buf, &mut fds)?;
    if fd_count == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "copyover handoff carried no listener fd",
        ));
    }
    // fds delivered via SCM_RIGHTS arrive WITHOUT close-on-exec. Set it now, or
    // they'd be inherited by *this* process's own future copyover successor
    // (fork+exec) as leaked duplicates — a second fd on the same socket that
    // never closes, so the client never sees EOF across a chained copyover.
    for fd in &fds[..fd_count] {
        let borrowed = unsafe { std::os::fd::BorrowedFd::borrow_raw(*fd) };
        let _ = rustix::io::fcntl_setfd(borrowed, rustix::io::FdFlags::CLOEXEC);
    }
    let manifest: HandoverManifest = serde_json::from_slice(&buf[..n]).map_err(invalid_data)?;
    let listener_fd = fds[0];
    let conn_fds = fds[1..fd_count].to_vec();
    Ok((manifest, listener_fd, conn_fds))
}

/// Predecessor side of a copyover: spawn the successor, hand it the listener +
/// client fds and the manifest over a unix socket, and wait for its ack.
pub(crate) fn perform_handoff(manifest: &HandoverManifest, fds: &[RawFd]) -> std::io::Result<()> {
    use std::io::Read;
    let sock_path = std::env::temp_dir().join(format!("grim-copyover-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path)?;

    let exe = current_exe_path()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.env(COPYOVER_SOCK_ENV, &sock_path);
    let mut child = cmd.spawn()?;

    // Everything after spawn can fail; on any failure the predecessor resumes
    // serving, so the half-started successor must not linger (dropping `Child`
    // does not kill it) holding dup'd sockets or the port.
    let outcome = (|| -> std::io::Result<()> {
        let (mut stream, _addr) = listener.accept()?;
        write_handoff(&stream, manifest, fds)?;

        // Wait for the successor to confirm it is serving before we let the
        // process exit — this keeps the fds (and the systemd MainPID handoff)
        // valid until the new instance has taken over.
        let mut ack = [0u8; 1];
        stream.read_exact(&mut ack)?;

        // Hand the systemd MainPID to the successor *from the current main
        // process* (us). systemd trusts a MAINPID change from the tracked main
        // most readily, and doing it here — before we exit — queues the
        // reassignment ahead of our exit rather than racing the successor's own
        // notify. The successor also sends MAINPID+READY; this is the
        // authoritative belt-and-suspenders. No-op outside systemd.
        let _ = sd_notify::notify(&[sd_notify::NotifyState::MainPid(child.id())]);
        Ok(())
    })();

    let _ = std::fs::remove_file(&sock_path);

    if let Err(e) = outcome {
        error!("copyover handoff failed ({e}); terminating half-started successor");
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    Ok(())
}

/// Successor side of a copyover: connect to the predecessor and receive the
/// listener + client fds plus the manifest. Blocking; run before the runtime.
pub(crate) fn receive_handoff(path: &str) -> std::io::Result<Handoff> {
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

// ─── Systems ────────────────────────────────────────────────────────

/// Wire `SIGUSR2` to the copyover flag. Failure is logged, not fatal.
pub(crate) fn install_copyover_signal(signal: Res<CopyoverSignal>) {
    match signal_hook::flag::register(signal_hook::consts::SIGUSR2, signal.0.clone()) {
        Ok(_) => info!("SIGUSR2 will trigger a copyover (hot restart)"),
        Err(e) => warn!("failed to register SIGUSR2 handler: {e}"),
    }
}

/// If `SIGUSR2` fired, snapshot the in-game connections and ask the tokio thread
/// to hand them to a successor process. Runs once — `started` latches so a
/// second signal mid-handoff is ignored.
pub(crate) fn poll_copyover_signal(
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
pub(crate) fn finish_copyover(done: Res<CopyoverDone>, mut exit: MessageWriter<AppExit>) {
    if done.0.load(Ordering::SeqCst) {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;

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
}
