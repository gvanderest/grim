//! Idle tracking + AFK: stamp session activity on every input line, warn and
//! disconnect long-idle sessions, auto-flag quiet in-game sessions AFK.
//!
//! Activity is stamped here — one pre-dispatch system watching `ConnectionInput`
//! — rather than in each dispatcher, so pre-game, in-game, and editor lines all
//! count with a single hook. Enforcement reuses the normal close path
//! (`DisconnectRequest` → `ConnectionClosed` → linkdead).

use std::time::Duration;

use bevy::prelude::*;
use grim_core::components::{Client, ClientState};
use grim_networking::{ConnectionInput, ConnectionOutput, DisconnectRequest};
use grim_text::tr;

/// Seconds-based idle thresholds. All fields are **seconds**; converted to
/// `Duration` at use. Inserted with defaults by `ScenePlugin` — an author
/// overrides by inserting a custom value before/after adding the plugin.
///
/// The two timeouts govern different states (AFK for in-game, disconnect for
/// pre-game), so they need no ordering between them. A `warn_lead` of `0` —
/// or at/above the disconnect timeout — disables the warning.
#[derive(Resource, Debug, Clone, Copy)]
pub struct IdleConfig {
    /// Quiet seconds before an in-game session auto-flags AFK. AFK is the
    /// ceiling unless `disconnect_ingame_idle` opts in below.
    pub afk_after_secs: u64,
    /// Quiet seconds before a session is disconnected: always for *pre-game*
    /// (login-limbo) sessions, and for in-game ones only when
    /// `disconnect_ingame_idle` is set.
    pub disconnect_after_secs: u64,
    /// Seconds before a disconnect that the warn-once notice fires.
    pub warn_secs_before_disconnect: u64,
    /// Opt-in: also sever in-game sessions for idleness (default `false` —
    /// players may idle in the world all day, AFK-flagged).
    pub disconnect_ingame_idle: bool,
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            afk_after_secs: 300,
            disconnect_after_secs: 1800,
            warn_secs_before_disconnect: 300,
            disconnect_ingame_idle: false,
        }
    }
}

/// Stamp activity for every input line: refresh `last_active`, reset the warn
/// latch, and silently clear `afk` (the prompt change + `who` marker are the
/// signal — no cleared notice). Unknown connections are skipped (fail closed).
/// Runs before both dispatchers (see `SceneSystems::TouchInput`).
pub(crate) fn touch_sessions_on_input(
    mut inputs: MessageReader<ConnectionInput>,
    mut clients: Query<&mut Client>,
    time: Res<Time>,
) {
    let now = time.elapsed();
    for ev in inputs.read() {
        let Some(mut client) = clients.iter_mut().find(|c| c.connection == ev.connection) else {
            continue;
        };
        client.last_active = Some(now);
        client.idle_warned = false;
        client.afk = false;
    }
}

/// Auto-flag quiet in-game sessions AFK (they stay connected as long as they
/// like — idling in the world is fine); warn then disconnect quiet *pre-game*
/// sessions (login limbo must not sit forever). Sessions never seen
/// (`last_active: None`) are stamped and skipped — every spawn site gets a
/// grace tick without touching `Client::new`.
pub(crate) fn check_idle(
    time: Res<Time>,
    config: Res<IdleConfig>,
    mut clients: Query<&mut Client>,
    mut outputs: MessageWriter<ConnectionOutput>,
    mut disconnect: MessageWriter<DisconnectRequest>,
) {
    let now = time.elapsed();
    let disconnect_after = Duration::from_secs(config.disconnect_after_secs);
    // A zero lead disables warnings; a lead at/above the timeout would warn
    // every session on first sighting, so it disables them too (fail closed).
    // Either way the unreachable `warn_at` keeps the branch dead.
    let warn_at = if config.warn_secs_before_disconnect == 0
        || config.warn_secs_before_disconnect >= config.disconnect_after_secs
    {
        disconnect_after
    } else {
        disconnect_after.saturating_sub(Duration::from_secs(config.warn_secs_before_disconnect))
    };
    let afk_after = Duration::from_secs(config.afk_after_secs);
    for mut client in clients.iter_mut() {
        let Some(last) = client.last_active else {
            client.last_active = Some(now);
            continue;
        };
        let idle = now.saturating_sub(last);
        let ingame = client.state == ClientState::InGame;
        if ingame && !client.afk && idle >= afk_after {
            client.afk = true;
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(client.connection, tr!("afk.set"))
            });
        }
        // In-game idlers stay connected unless the author opted in;
        // login-limbo always severs (bots must not sit forever).
        if ingame && !config.disconnect_ingame_idle {
            continue;
        }
        if idle >= disconnect_after {
            let conn = client.connection;
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(conn, tr!("idle.disconnect"))
            });
            disconnect.write(DisconnectRequest { connection: conn });
            continue;
        }
        if !client.idle_warned && idle >= warn_at {
            client.idle_warned = true;
            let conn = client.connection;
            let idle_minutes = (idle.as_secs() / 60).to_string();
            let left_minutes =
                (disconnect_after.saturating_sub(idle).as_secs().div_ceil(60)).to_string();
            outputs.write(ConnectionOutput {
                echo: None,
                ..ConnectionOutput::new(
                    conn,
                    tr!(
                        "idle.warn",
                        idle_minutes = idle_minutes,
                        left_minutes = left_minutes
                    ),
                )
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grim_networking::Connection;

    fn test_app() -> App {
        // Bare app (no TimePlugin) so the tests own the clock deterministically
        // via `advance_by` — the automatic time update would overwrite it.
        let mut app = App::new();
        app.init_resource::<Time>();
        app.init_resource::<IdleConfig>();
        app.add_message::<ConnectionInput>();
        app.add_message::<ConnectionOutput>();
        app.add_message::<DisconnectRequest>();
        app.add_systems(Update, (touch_sessions_on_input, check_idle));
        app
    }

    fn spawn_session(app: &mut App, last_active: Option<Duration>) -> (Entity, Entity) {
        let conn_e = app
            .world_mut()
            .spawn(Connection {
                id: 1,
                addr: "127.0.0.1:40001".parse().unwrap(),
                echo_hidden: false,
            })
            .id();
        let mut client = Client::new(conn_e);
        client.state = ClientState::InGame;
        client.last_active = last_active;
        let session = app.world_mut().spawn(client).id();
        (session, conn_e)
    }

    fn outputs(app: &App) -> Vec<String> {
        let messages = app.world().resource::<Messages<ConnectionOutput>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|o| o.text.clone()).collect()
    }

    fn disconnects(app: &App) -> Vec<Entity> {
        let messages = app.world().resource::<Messages<DisconnectRequest>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|m| m.connection).collect()
    }

    #[test]
    fn input_stamps_activity_and_clears_afk() {
        let mut app = test_app();
        let (session, conn) = spawn_session(&mut app, None);
        app.world_mut().get_mut::<Client>(session).unwrap().afk = true;
        app.world_mut().write_message(ConnectionInput {
            connection: conn,
            text: "look".into(),
        });
        app.update();
        let client = app.world().get::<Client>(session).unwrap();
        assert_eq!(client.last_active, Some(Duration::ZERO));
        assert!(!client.afk);
    }

    #[test]
    fn quiet_session_warns_once_then_disconnects() {
        let mut app = test_app();
        app.insert_resource(IdleConfig {
            afk_after_secs: 100_000,
            disconnect_after_secs: 3600,
            warn_secs_before_disconnect: 600,
            disconnect_ingame_idle: true,
        });
        let (_, conn) = spawn_session(&mut app, Some(Duration::ZERO));
        // 3060s idle: past warn_at (3000s), short of disconnect (3600s).
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(3060));
        app.update();
        assert_eq!(outputs(&app).len(), 1);
        assert!(outputs(&app)[0].contains("51 minutes"));
        assert!(disconnects(&app).is_empty());
        // Same state re-run without advancing: latch holds, no second warn.
        app.update();
        assert_eq!(outputs(&app).len(), 1);
        // Past disconnect: severed with the disconnect notice.
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(600));
        app.update();
        assert_eq!(disconnects(&app), vec![conn]);
        assert!(outputs(&app).iter().any(|t| t.contains("idle too long")));
    }

    #[test]
    fn oversized_warn_lead_disables_warnings() {
        let mut app = test_app();
        app.insert_resource(IdleConfig {
            afk_after_secs: 100_000,
            disconnect_after_secs: 60,
            warn_secs_before_disconnect: 120,
            disconnect_ingame_idle: true,
        });
        spawn_session(&mut app, Some(Duration::ZERO));
        // Idle 30s: past where warn_at would saturate (0s) under the naive
        // subtraction, but the clamp disables warnings — stays silent.
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(30));
        app.update();
        assert!(outputs(&app).is_empty());
        assert!(disconnects(&app).is_empty());
    }

    #[test]
    fn quiet_ingame_session_auto_flags_afk() {
        let mut app = test_app();
        app.insert_resource(IdleConfig {
            afk_after_secs: 60,
            disconnect_after_secs: 70,
            warn_secs_before_disconnect: 0,
            disconnect_ingame_idle: false,
        });
        let (session, _) = spawn_session(&mut app, Some(Duration::ZERO));
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(61));
        app.update();
        assert!(app.world().get::<Client>(session).unwrap().afk);
        assert!(outputs(&app).iter().any(|t| t.contains("AFK")));
        // Past the disconnect threshold with the opt-out default: still
        // connected, no warn, no sever — AFK is the ceiling in the world.
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(20));
        app.update();
        assert!(disconnects(&app).is_empty());
        assert!(outputs(&app).iter().all(|t| !t.contains("idle")));
    }
}
