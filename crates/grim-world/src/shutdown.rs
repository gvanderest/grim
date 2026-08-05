//! Graceful server shutdown, triggered two ways:
//!
//! - **In-game:** `shutdown <seconds>` from an admin character.
//! - **Out-of-band:** `SIGTERM` to the process — this is what `systemctl stop`
//!   sends, so a stop/restart warns players instead of terminating abruptly. No
//!   login or admin credentials are involved. (Copyover, a *hot* restart that
//!   keeps players connected, uses `SIGUSR2` instead — see `grim-networking-telnet`.)
//!
//! Either path schedules the same countdown: every connected player is warned
//! at decreasing intervals, and when it expires the app writes
//! [`AppExit::Success`] (exit code 0). The systemd unit uses `Restart=on-failure`,
//! so a clean exit stays down and lets the deploy swap the binary before
//! restarting — see `docs/DEPLOY.md`.
//!
//! Player state is **not** flushed on shutdown: characters save on disconnect
//! and on `quit`, and the project currently tolerates losing in-flight position
//! changes across a restart.

use bevy::prelude::*;
use grim_engine_types::components::Character;
use grim_engine_types::events::{Command, EngineCommand, InfoMessage, ServerBroadcast};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Countdown used when the shutdown is triggered by `SIGTERM` (the systemctl-stop/deploy path).
const SIGNAL_COUNTDOWN_SECS: u64 = 30;

/// Countdown thresholds (seconds remaining) at which a warning is broadcast.
/// Only thresholds strictly below the requested duration fire.
const WARN_AT: [u64; 6] = [30, 15, 10, 5, 3, 1];

/// Pure countdown state. Kept free of Bevy types so the timing logic is unit
/// testable without a running `App`.
#[derive(Debug, Clone, PartialEq)]
pub struct ShutdownCountdown {
    remaining: f32,
    /// Warning thresholds not yet announced, in descending order.
    pending: Vec<u64>,
}

/// What a single [`ShutdownCountdown::advance`] produced.
#[derive(Debug, Default, PartialEq)]
pub struct Tick {
    /// Thresholds (seconds) crossed this tick, largest first.
    pub warnings: Vec<u64>,
    /// The countdown reached zero this tick or earlier.
    pub expired: bool,
}

impl ShutdownCountdown {
    /// Start a countdown of `seconds`.
    pub fn new(seconds: u64) -> Self {
        let mut pending: Vec<u64> = WARN_AT.iter().copied().filter(|&t| t < seconds).collect();
        pending.sort_unstable_by(|a, b| b.cmp(a)); // descending
        Self {
            remaining: seconds as f32,
            pending,
        }
    }

    /// Advance by `dt` seconds, returning any warnings crossed and whether the
    /// countdown has expired. Once expired, no further warnings are produced.
    pub fn advance(&mut self, dt: f32) -> Tick {
        self.remaining -= dt;
        if self.remaining <= 0.0 {
            self.pending.clear();
            return Tick {
                warnings: Vec::new(),
                expired: true,
            };
        }
        let mut warnings = Vec::new();
        while let Some(&t) = self.pending.first() {
            if self.remaining <= t as f32 {
                warnings.push(t);
                self.pending.remove(0);
            } else {
                break;
            }
        }
        Tick {
            warnings,
            expired: false,
        }
    }
}

/// Present while a shutdown is counting down. Absence means no shutdown pending.
#[derive(Resource, Debug)]
pub struct ActiveShutdown(pub ShutdownCountdown);

/// Shared flag set by the `SIGTERM` handler and drained by `poll_shutdown_signal`.
/// A signal handler can do almost nothing safely, so it only flips this bool; the
/// real work happens on the next Bevy tick.
#[derive(Resource, Clone)]
struct ShutdownSignal(Arc<AtomicBool>);

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}

/// Registers the shutdown message + systems.
pub struct ShutdownPlugin;

impl Plugin for ShutdownPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ServerBroadcast>()
            .init_resource::<ShutdownSignal>()
            .add_systems(Startup, install_signal_handler)
            // Chained so the sync point between them applies each system's
            // `insert_resource(ActiveShutdown)` before the next reads it —
            // otherwise a SIGTERM and an admin `shutdown` in the same tick both
            // observe no active shutdown and schedule conflicting countdowns.
            .add_systems(
                Update,
                (poll_shutdown_signal, handle_shutdown_command, tick_shutdown).chain(),
            );
    }
}

/// Wire `SIGTERM` to the shared flag so `systemctl stop` (and the deploy) trigger
/// a warned, graceful shutdown instead of an abrupt terminate. Failure is logged,
/// not fatal — the in-game `shutdown` command still works without it. The systemd
/// unit's `TimeoutStopSec` must exceed the countdown, else systemd `SIGKILL`s
/// mid-countdown. (Copyover uses `SIGUSR2` and is handled by the telnet transport.)
fn install_signal_handler(signal: Res<ShutdownSignal>) {
    match signal_hook::flag::register(signal_hook::consts::SIGTERM, signal.0.clone()) {
        Ok(_) => info!("SIGTERM will trigger a {SIGNAL_COUNTDOWN_SECS}s graceful shutdown"),
        Err(e) => warn!("failed to register SIGTERM handler: {e}"),
    }
}

/// If `SIGTERM` fired, start the countdown (unless one is already running).
fn poll_shutdown_signal(
    signal: Res<ShutdownSignal>,
    active: Option<Res<ActiveShutdown>>,
    mut broadcast: MessageWriter<ServerBroadcast>,
    mut commands: Commands,
) {
    if !signal.0.swap(false, Ordering::SeqCst) {
        return;
    }
    if active.is_some() {
        return;
    }
    broadcast.write(ServerBroadcast {
        text: warn_text(SIGNAL_COUNTDOWN_SECS),
    });
    commands.insert_resource(ActiveShutdown(ShutdownCountdown::new(
        SIGNAL_COUNTDOWN_SECS,
    )));
}

fn warn_text(seconds: u64) -> String {
    format!("{{R[SERVER]{{x The server is restarting in {{Y{seconds}{{x seconds.\n")
}

/// `shutdown <seconds>`: admin-gated (defense in depth — the client gates first).
/// Non-admins are ignored silently; a second request while one is pending is
/// rejected.
fn handle_shutdown_command(
    mut engine: MessageReader<EngineCommand>,
    characters: Query<&Character>,
    active: Option<Res<ActiveShutdown>>,
    mut info: MessageWriter<InfoMessage>,
    mut broadcast: MessageWriter<ServerBroadcast>,
    mut commands: Commands,
) {
    for cmd in engine.read() {
        let Command::Shutdown { seconds } = cmd.command else {
            continue;
        };
        let actor = cmd.client;
        // Defense in depth. The client already gates `shutdown` and masks it as
        // an unknown command for non-admins, so a well-behaved session never
        // sends this for a non-admin. If one arrives anyway (a non-client
        // command source), fail closed and stay silent — emitting anything here
        // would leak the command's existence with the wrong output framing.
        let is_admin = characters
            .get(actor)
            .map(Character::is_admin)
            .unwrap_or(false);
        if !is_admin {
            continue;
        }
        if active.is_some() {
            info.write(InfoMessage {
                target: actor,
                text: "A shutdown is already scheduled.\n".into(),
            });
            continue;
        }
        broadcast.write(ServerBroadcast {
            text: warn_text(seconds),
        });
        commands.insert_resource(ActiveShutdown(ShutdownCountdown::new(seconds)));
    }
}

/// Ticks the active countdown, emitting warnings and finally `AppExit`.
fn tick_shutdown(
    time: Res<Time>,
    active: Option<ResMut<ActiveShutdown>>,
    mut broadcast: MessageWriter<ServerBroadcast>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut active) = active else {
        return;
    };
    let tick = active.0.advance(time.delta_secs());
    for secs in tick.warnings {
        broadcast.write(ServerBroadcast {
            text: warn_text(secs),
        });
    }
    if tick.expired {
        broadcast.write(ServerBroadcast {
            text: "{R[SERVER]{x The server is restarting now.\n".into(),
        });
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_only_keeps_thresholds_below_duration() {
        let cd = ShutdownCountdown::new(30);
        // 30 is not < 30, so it is excluded; the rest descend.
        assert_eq!(cd.pending, vec![15, 10, 5, 3, 1]);
    }

    #[test]
    fn short_countdown_has_few_warnings() {
        let cd = ShutdownCountdown::new(4);
        assert_eq!(cd.pending, vec![3, 1]);
    }

    #[test]
    fn advance_crosses_single_threshold() {
        let mut cd = ShutdownCountdown::new(30);
        // Down to 16: no threshold yet.
        assert_eq!(cd.advance(14.0), Tick::default());
        // Down to 14: crosses 15.
        let t = cd.advance(2.0);
        assert_eq!(t.warnings, vec![15]);
        assert!(!t.expired);
    }

    #[test]
    fn advance_crosses_multiple_thresholds_in_one_tick() {
        let mut cd = ShutdownCountdown::new(30);
        // Jump straight to 4 remaining: crosses 15, 10, 5 at once.
        let t = cd.advance(26.0);
        assert_eq!(t.warnings, vec![15, 10, 5]);
        assert!(!t.expired);
    }

    #[test]
    fn advance_expires_without_spurious_warnings() {
        let mut cd = ShutdownCountdown::new(2);
        let t = cd.advance(5.0);
        assert!(t.expired);
        assert!(t.warnings.is_empty());
    }

    #[test]
    fn expiry_is_sticky_and_at_exact_zero() {
        let mut cd = ShutdownCountdown::new(1);
        let t = cd.advance(1.0);
        assert!(t.expired);
    }

    #[test]
    fn warn_text_contains_count_and_colour() {
        let s = warn_text(15);
        assert!(s.contains("15"));
        assert!(s.starts_with("{R[SERVER]"));
        assert!(s.ends_with('\n'));
    }

    // ── System-level tests ────────────────────────────────────────

    use chrono::Utc;
    use grim_engine_types::components::{Gender, Role};
    use grim_engine_types::GrimId;
    use std::time::Duration;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ShutdownPlugin);
        app.add_message::<EngineCommand>()
            .add_message::<InfoMessage>();
        app
    }

    fn spawn_character(app: &mut App, roles: Vec<Role>) -> Entity {
        app.world_mut()
            .spawn(Character {
                id: GrimId::new(),
                name: "Tester".into(),
                account_id: GrimId::new(),
                created_at: Utc::now(),
                last_room: None,
                roles,
                gender: Gender::Neutral,
                race: String::new(),
                class: String::new(),
                level: 1,
            })
            .id()
    }

    fn drain<M: Message + std::fmt::Debug>(app: &App) -> Vec<String> {
        let messages = app.world().resource::<Messages<M>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|m| format!("{m:?}")).collect()
    }

    #[test]
    fn non_admin_is_denied_and_nothing_scheduled() {
        let mut app = test_app();
        let actor = spawn_character(&mut app, Vec::new());
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Shutdown { seconds: 30 },
        });
        app.update();

        // Silent fail-closed: no schedule, and no output at all (the client
        // owns the unknown-command masking; the engine must not emit anything
        // that would leak the command's existence).
        assert!(app.world().get_resource::<ActiveShutdown>().is_none());
        assert_eq!(drain::<InfoMessage>(&app).len(), 0);
        assert_eq!(drain::<ServerBroadcast>(&app).len(), 0);
    }

    #[test]
    fn admin_schedules_and_broadcasts() {
        let mut app = test_app();
        let actor = spawn_character(&mut app, vec![Role::Admin]);
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Shutdown { seconds: 30 },
        });
        app.update();

        assert!(app.world().get_resource::<ActiveShutdown>().is_some());
        let casts = drain::<ServerBroadcast>(&app);
        assert!(casts.iter().any(|c| c.contains("30")));
    }

    #[test]
    fn sigterm_flag_triggers_countdown() {
        let mut app = test_app();
        app.update(); // Startup installs the handler.
        let _ = drain::<ServerBroadcast>(&app);

        // Simulate the signal handler firing.
        app.world()
            .resource::<ShutdownSignal>()
            .0
            .store(true, Ordering::SeqCst);
        app.update();

        assert!(app.world().get_resource::<ActiveShutdown>().is_some());
        let casts = drain::<ServerBroadcast>(&app);
        assert!(casts.iter().any(|c| c.contains("30")));
    }

    #[test]
    fn sigterm_ignored_when_already_counting_down() {
        let mut app = test_app();
        app.update();
        app.world_mut()
            .insert_resource(ActiveShutdown(ShutdownCountdown::new(30)));
        let _ = drain::<ServerBroadcast>(&app);

        app.world()
            .resource::<ShutdownSignal>()
            .0
            .store(true, Ordering::SeqCst);
        app.update();

        // Signal drained but no fresh countdown/broadcast.
        assert!(drain::<ServerBroadcast>(&app).is_empty());
    }

    #[test]
    fn second_shutdown_is_rejected() {
        let mut app = test_app();
        let actor = spawn_character(&mut app, vec![Role::Admin]);
        app.world_mut()
            .insert_resource(ActiveShutdown(ShutdownCountdown::new(30)));
        app.world_mut().write_message(EngineCommand {
            client: actor,
            command: Command::Shutdown { seconds: 10 },
        });
        app.update();

        let infos = drain::<InfoMessage>(&app);
        assert!(infos.iter().any(|i| i.contains("already scheduled")));
    }

    #[test]
    fn expiry_writes_app_exit() {
        let mut app = test_app();
        app.world_mut()
            .insert_resource(ActiveShutdown(ShutdownCountdown::new(0)));
        app.update();

        let exits = drain::<AppExit>(&app);
        assert_eq!(exits.len(), 1);
        let casts = drain::<ServerBroadcast>(&app);
        assert!(casts.iter().any(|c| c.contains("now")));
    }

    #[test]
    fn tick_broadcasts_threshold_warning() {
        // Bare app (no TimePlugin) so we own the clock deterministically.
        let mut app = App::new();
        app.add_plugins(ShutdownPlugin);
        app.add_message::<EngineCommand>()
            .add_message::<InfoMessage>();
        app.init_resource::<Time>();
        app.insert_resource(ActiveShutdown(ShutdownCountdown::new(16)));

        // Advance the clock 1.5s: 16 → 14.5 remaining, crossing the 15s mark.
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(1500));
        app.update();

        let casts = drain::<ServerBroadcast>(&app);
        assert!(
            casts.iter().any(|c| c.contains("15")),
            "expected a 15s warning, got {casts:?}"
        );
    }

    #[test]
    fn signal_and_command_in_same_tick_schedule_once() {
        let mut app = test_app();
        let admin = spawn_character(&mut app, vec![Role::Admin]);
        app.update(); // Startup; drain the install-time state.
        let _ = drain::<ServerBroadcast>(&app);

        // Both triggers arrive before a single update.
        app.world()
            .resource::<ShutdownSignal>()
            .0
            .store(true, Ordering::SeqCst);
        app.world_mut().write_message(EngineCommand {
            client: admin,
            command: Command::Shutdown { seconds: 10 },
        });
        app.update();

        // The chained sync point means the second trigger sees the first's
        // ActiveShutdown, so exactly one countdown is scheduled and one warning
        // is broadcast (not two conflicting ones).
        assert!(app.world().get_resource::<ActiveShutdown>().is_some());
        assert_eq!(drain::<ServerBroadcast>(&app).len(), 1);
    }
}
