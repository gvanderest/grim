//! Real-network copyover (hot restart) test.
//!
//! Unlike the message-level harness in `scenarios.rs`, copyover cannot be
//! exercised in-process: it forks + execs the compiled binary and passes live
//! socket fds over a unix socket (SCM_RIGHTS). So this test spawns the real
//! `copyover_fixture` binary, drives it as a telnet client would, triggers a
//! copyover with `SIGUSR2`, and asserts the *same* TCP connection survives and
//! resumes the character in the room it walked to — no re-login. Nothing more.
//!
//! Unix-only: copyover relies on POSIX signals + fd passing.
#![cfg(unix)]

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A password that satisfies validation.
const PW: &str = "secretpw";

/// Read from `stream` until `needle` appears in the accumulated output or
/// `timeout` elapses. Returns everything read (lossy UTF-8, telnet IAC bytes and
/// all). Panics on timeout, dumping what was seen — that is the failure signal.
fn expect(stream: &mut TcpStream, needle: &str, timeout: Duration) -> String {
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let deadline = Instant::now() + timeout;
    let mut acc = String::new();
    let mut buf = [0u8; 4096];
    while Instant::now() < deadline {
        match stream.read(&mut buf) {
            Ok(0) => break, // peer closed
            Ok(n) => {
                acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                if acc.contains(needle) {
                    return acc;
                }
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => panic!("read error waiting for {needle:?}: {e}"),
        }
    }
    panic!("timed out waiting for {needle:?}; saw:\n{acc}");
}

fn send(stream: &mut TcpStream, line: &str) {
    stream.write_all(line.as_bytes()).unwrap();
    stream.write_all(b"\n").unwrap();
    stream.flush().unwrap();
}

/// Connect with retries while the fixture's listener comes up.
fn connect(port: u16) -> TcpStream {
    let addr = ("127.0.0.1", port)
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(300)) {
            Ok(s) => return s,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => panic!("fixture never accepted connections: {e}"),
        }
    }
}

/// `SIGKILL` the whole process group (fixture + any copyover successor it forked)
/// so no server outlives the test.
fn kill_group(pgid: u32) {
    let _ = Command::new("kill")
        .arg("-KILL")
        .arg(format!("-{pgid}"))
        .status();
}

#[test]
fn copyover_keeps_player_connected_and_resumes_last_room() {
    // Unique port + isolated data dir per run.
    let port: u16 = 40000 + (std::process::id() % 10000) as u16;
    let dir = std::env::temp_dir().join(format!("grim-copyover-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Run from a *copy* of the binary so we can replace it before the copyover,
    // reproducing a real deploy: the swap unlinks the running image, so the
    // successor's exec path must survive the `(deleted)` marker Linux reports.
    let bin = dir.join("grim-server");
    std::fs::copy(env!("CARGO_BIN_EXE_copyover_fixture"), &bin).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut child: Child = Command::new(&bin)
        .env("GRIM_TEST_PORT", port.to_string())
        .env("GRIM_TEST_DATA", &dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Own process group so we can reap the fixture *and* its successor.
        .process_group(0)
        .spawn()
        .expect("spawn copyover_fixture");
    let pgid = child.id();

    // Guard so a panic mid-test still tears the server down.
    struct Cleanup(u32);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            kill_group(self.0);
        }
    }
    let _cleanup = Cleanup(pgid);

    let mut stream = connect(port);

    // ── Create an account + character, enter the world (in the tavern) ──
    expect(
        &mut stream,
        "character name or email",
        Duration::from_secs(20),
    );
    send(&mut stream, "alice@example.com");
    expect(&mut stream, "create an account", Duration::from_secs(20));
    send(&mut stream, "y");
    expect(&mut stream, "Choose a password", Duration::from_secs(20));
    send(&mut stream, PW);
    // Account created → character menu. Create a character.
    expect(&mut stream, "character", Duration::from_secs(20));
    send(&mut stream, "c");
    expect(
        &mut stream,
        "name for your new character",
        Duration::from_secs(20),
    );
    send(&mut stream, "Alice");
    // MOTD → press enter to enter the world → land in the starting tavern.
    std::thread::sleep(Duration::from_millis(300));
    send(&mut stream, "");
    expect(&mut stream, "Exits:", Duration::from_secs(20));

    // ── Walk north into the Town Square (in-game; respect command cooldown) ──
    std::thread::sleep(Duration::from_millis(1500));
    send(&mut stream, "north");
    expect(&mut stream, "Town Square", Duration::from_secs(20));

    // ── Swap the binary on disk (as a deploy would), then trigger copyover ──
    // Overwrite via a temp file + rename so the running image's inode is unlinked
    // — this is what makes `current_exe()` report a `(deleted)` path.
    let staged = dir.join("grim-server.new");
    std::fs::copy(env!("CARGO_BIN_EXE_copyover_fixture"), &staged).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::rename(&staged, &bin).unwrap();

    let status = Command::new("kill")
        .arg("-USR2")
        .arg(pgid.to_string())
        .status()
        .expect("send SIGUSR2");
    assert!(status.success(), "kill -USR2 failed");

    // The predecessor exits; reap it so it isn't a zombie. The successor (in the
    // same process group) is now serving on our still-open socket.
    let _ = child.wait();

    // ── Same socket, no re-login: greeted by the reload and back in the Square ──
    expect(&mut stream, "world was reloaded", Duration::from_secs(60));

    std::thread::sleep(Duration::from_millis(1500));
    send(&mut stream, "look");
    let after = expect(&mut stream, "Town Square", Duration::from_secs(20));
    assert!(
        !after.contains("Rusted Anvil"),
        "should have resumed in the Town Square (walked-to room), not the tavern"
    );

    // ── Second, chained copyover (successor → successor) ──
    // The original process is gone; signal the whole process group so whichever
    // process is currently serving receives it.
    let staged2 = dir.join("grim-server.new2");
    std::fs::copy(env!("CARGO_BIN_EXE_copyover_fixture"), &staged2).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged2, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::rename(&staged2, &bin).unwrap();
    let status = Command::new("kill")
        .arg("-USR2")
        .arg(format!("-{pgid}"))
        .status()
        .expect("send SIGUSR2 to group");
    assert!(status.success(), "second kill -USR2 failed");
    expect(&mut stream, "world was reloaded", Duration::from_secs(60));
    std::thread::sleep(Duration::from_millis(1500));
    send(&mut stream, "look");
    expect(&mut stream, "Town Square", Duration::from_secs(20));

    // ── `quit` after a copyover must still close the connection ──
    std::thread::sleep(Duration::from_millis(1500));
    send(&mut stream, "quit");
    // Server-side close → our reads return 0 (EOF) within a moment.
    stream
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut closed = false;
    let mut buf = [0u8; 1024];
    while Instant::now() < deadline {
        match stream.read(&mut buf) {
            Ok(0) => {
                closed = true;
                break;
            }
            Ok(_) => {}
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => {
                closed = true;
                break;
            }
        }
    }
    assert!(
        closed,
        "`quit` after a (chained) copyover must disconnect the socket — a \
         non-CLOEXEC fd inherited across the fork+exec previously kept it open"
    );

    // _cleanup drops here, killing the successor.
    let _ = std::fs::remove_dir_all(&dir);
}
