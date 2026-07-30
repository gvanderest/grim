//! Headless end-to-end harness for example-mud.
//!
//! Boots the real game stack (`GrimHeadlessPlugins`) with the real world seed,
//! but no transport: input is injected as `ConnectionInput` and every
//! `ConnectionOutput` line is recorded per connection. Each `Mud::new()` gets a
//! fresh temp data directory, so tests start from a clean slate and never touch
//! the repo's `data/`.
//!
//! ```ignore
//! let mut mud = Mud::new();
//! let (alice, banner) = mud.connect();
//! banner.assert_contains("character name or email");
//! mud.send(alice, "alice@example.com").assert_contains("create an account");
//! ```

#![allow(dead_code)]

use bevy::prelude::*;
use grim::components::{Character, Name as GrimName};
use grim::plugins::PersistenceConfig;
use grim::GrimHeadlessPlugins;
use grim::{
    Connection, ConnectionClosed, ConnectionEstablished, ConnectionInput, ConnectionOutput,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Unique suffix per harness instance so concurrent tests get distinct temp dirs.
static INSTANCE: AtomicUsize = AtomicUsize::new(0);

/// A connected session — a handle to one simulated telnet user.
#[derive(Clone, Copy)]
pub struct Session {
    pub conn: Entity,
}

/// Output produced in response to a single interaction, with content assertions.
#[must_use]
pub struct Output {
    text: String,
}

impl Output {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn contains(&self, needle: &str) -> bool {
        self.text.contains(needle)
    }

    /// Assert the output contains `needle`; panics with the full transcript.
    pub fn assert_contains(&self, needle: &str) -> &Self {
        assert!(
            self.text.contains(needle),
            "expected output to contain {needle:?}, got:\n{}",
            self.text
        );
        self
    }

    /// Assert the output does NOT contain `needle`.
    pub fn assert_excludes(&self, needle: &str) -> &Self {
        assert!(
            !self.text.contains(needle),
            "expected output to NOT contain {needle:?}, got:\n{}",
            self.text
        );
        self
    }
}

pub struct Mud {
    app: App,
    next_conn: usize,
    data_dir: PathBuf,
    cursor: bevy::ecs::message::MessageCursor<ConnectionOutput>,
    buffers: HashMap<Entity, Vec<String>>,
    read_offsets: HashMap<Entity, usize>,
}

impl Mud {
    /// Boot a fresh MUD with an isolated temp data directory and the real seed.
    pub fn new() -> Self {
        let id = INSTANCE.fetch_add(1, Ordering::Relaxed);
        let data_dir = std::env::temp_dir().join(format!("grim-e2e-{}-{}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&data_dir);

        let mut app = App::new();
        app.insert_resource(PersistenceConfig {
            dir: data_dir.clone(),
        });
        // Manual clock (no TimePlugin) so per-command cooldowns advance
        // deterministically in `pump`.
        app.init_resource::<Time>();
        app.add_plugins(GrimHeadlessPlugins);
        app.add_systems(Startup, example_mud::seed::seed_world);

        // Run startup (seed + persistence load) and grab an output cursor before
        // any connection exists.
        app.update();
        let cursor = app
            .world()
            .resource::<Messages<ConnectionOutput>>()
            .get_cursor();

        Self {
            app,
            next_conn: 1,
            data_dir,
            cursor,
            buffers: HashMap::new(),
            read_offsets: HashMap::new(),
        }
    }

    /// Advance one frame, recording any output. Called several times per
    /// interaction so multi-hop dispatch and the command cooldown settle.
    fn tick(&mut self) {
        {
            let mut time = self.app.world_mut().resource_mut::<Time>();
            time.advance_by(Duration::from_secs(1));
        }
        self.app.update();
        let collected: Vec<(Entity, String)> = {
            let msgs = self.app.world().resource::<Messages<ConnectionOutput>>();
            self.cursor
                .read(msgs)
                .map(|o| (o.connection, o.text.clone()))
                .collect()
        };
        for (conn, text) in collected {
            self.buffers.entry(conn).or_default().push(text);
        }
    }

    /// Settle the world over several frames (enough for a queued command's
    /// cooldown to elapse and its output to be produced).
    fn pump(&mut self) {
        for _ in 0..8 {
            self.tick();
        }
    }

    /// Return output for `conn` recorded since it was last read, advancing the
    /// per-connection read cursor. So passive receipt (another player's speech)
    /// and command responses share one position.
    fn collect_new(&mut self, conn: Entity) -> Output {
        let len = self.buffers.get(&conn).map(|b| b.len()).unwrap_or(0);
        let from = self.read_offsets.get(&conn).copied().unwrap_or(0);
        let text = self
            .buffers
            .get(&conn)
            .map(|b| b[from.min(len)..].join(""))
            .unwrap_or_default();
        self.read_offsets.insert(conn, len);
        Output { text }
    }

    /// Open a new connection. Returns the session and the initial output
    /// (login banner + prompt).
    pub fn connect(&mut self) -> (Session, Output) {
        let id = self.next_conn;
        self.next_conn += 1;
        let addr: SocketAddr = format!("127.0.0.1:{}", 40000 + id).parse().unwrap();
        let conn = self
            .app
            .world_mut()
            .spawn(Connection {
                id,
                addr,
                echo_hidden: false,
            })
            .id();
        self.app.world_mut().write_message(ConnectionEstablished {
            connection: conn,
            addr,
        });
        self.pump();
        (Session { conn }, self.collect_new(conn))
    }

    /// Send a line of input as this session and return the resulting output.
    pub fn send(&mut self, session: Session, line: &str) -> Output {
        self.app.world_mut().write_message(ConnectionInput {
            connection: session.conn,
            text: line.to_string(),
        });
        self.pump();
        self.collect_new(session.conn)
    }

    /// Advance the world and return any output this session received passively
    /// (e.g. another player's speech), without sending anything.
    pub fn recv(&mut self, session: Session) -> Output {
        self.pump();
        self.collect_new(session.conn)
    }

    /// Drop the connection (as a socket close would), triggering save-on-disconnect.
    pub fn disconnect(&mut self, session: Session) {
        self.app.world_mut().write_message(ConnectionClosed {
            connection: session.conn,
        });
        self.pump();
    }

    /// Names of player characters currently in the world (for assertions about
    /// state). Filtered to entities with a `Character`, so seeded rooms and NPCs
    /// (which also carry a name) are excluded.
    pub fn character_names(&mut self) -> Vec<String> {
        let mut q = self.app.world_mut().query::<(&GrimName, &Character)>();
        let mut names: Vec<String> = q
            .iter(self.app.world())
            .map(|(name, _)| name.0.clone())
            .collect();
        names.sort();
        names
    }
}

impl Default for Mud {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Mud {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}
