//! Graceful server shutdown, triggered two ways:
//!
//! - **In-game:** `shutdown <seconds>` from an admin character. That handler
//!   reads a being (the actor's `Character`), so it lives in `grim-actor` and
//!   slots into [`ShutdownSet::Command`]; everything else here is being-free.
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
use grim_core::events::ServerBroadcast;
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

/// Ordering seam for the shutdown pipeline within `Update`. The admin `shutdown`
/// command handler lives in `grim-actor` (it reads a being — the actor's
/// `Character`); it slots into [`ShutdownSet::Command`], between this crate's
/// SIGTERM poll and countdown tick. Chaining the three sets means a SIGTERM and
/// an admin `shutdown` arriving in the same tick still schedule exactly one
/// countdown: the sync point between `Poll` and `Command` makes the command see
/// the poll's `ActiveShutdown` insert (and vice-versa) rather than both
/// observing "none pending" and scheduling conflicting countdowns.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ShutdownSet {
    /// SIGTERM poll — may start a countdown.
    Poll,
    /// Admin `shutdown` command (in `grim-actor`) — may start a countdown.
    Command,
    /// Advance the active countdown.
    Tick,
}

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
            // Chain the three phases so the sync point between them applies each
            // phase's `insert_resource(ActiveShutdown)` before the next reads it.
            // `Command` (the admin `shutdown` handler) lives in `grim-actor` and
            // slots into the middle; here it is an empty set unless that plugin
            // is composed. See [`ShutdownSet`].
            .configure_sets(
                Update,
                (ShutdownSet::Poll, ShutdownSet::Command, ShutdownSet::Tick).chain(),
            )
            .add_systems(Update, poll_shutdown_signal.in_set(ShutdownSet::Poll))
            .add_systems(Update, tick_shutdown.in_set(ShutdownSet::Tick));
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

/// The countdown warning line for `seconds` remaining. `pub` so `grim-actor`'s
/// admin `shutdown` command emits the identical text as the SIGTERM path.
pub fn warn_text(seconds: u64) -> String {
    format!("{{R[SERVER]{{x The server is restarting in {{Y{seconds}{{x seconds.\n")
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
    //
    // These cover the being-free half that stays here: the SIGTERM poll, the
    // countdown tick, and the cross-set ordering. The admin `shutdown` command
    // handler (which reads a `Character`) moved to `grim-actor`, so its
    // admin-gate / already-scheduled tests live there.

    use std::time::Duration;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(ShutdownPlugin);
        app
    }

    fn drain<M: Message + std::fmt::Debug>(app: &App) -> Vec<String> {
        let messages = app.world().resource::<Messages<M>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|m| format!("{m:?}")).collect()
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

    /// The cross-set chain (`Poll` → `Command` → `Tick`) must apply the poll's
    /// `insert_resource(ActiveShutdown)` before a `Command`-set system runs, so a
    /// SIGTERM and a same-tick command scheduler do not both schedule. A probe
    /// system standing in for `grim-actor`'s command verifies the sync point:
    /// firing the signal, it observes the poll's schedule already present and so
    /// does not schedule a second, conflicting countdown.
    #[test]
    fn command_set_sees_poll_schedule_via_chain() {
        #[derive(Resource, Default)]
        struct ProbeRequest(bool);

        // Mimics the actor command: in the Command set, if asked and nothing is
        // already scheduled, schedule + broadcast. When the chain works it sees
        // the poll's insert and stays quiet.
        fn probe(
            request: Res<ProbeRequest>,
            active: Option<Res<ActiveShutdown>>,
            mut broadcast: MessageWriter<ServerBroadcast>,
            mut commands: Commands,
        ) {
            if !request.0 || active.is_some() {
                return;
            }
            broadcast.write(ServerBroadcast {
                text: warn_text(10),
            });
            commands.insert_resource(ActiveShutdown(ShutdownCountdown::new(10)));
        }

        let mut app = test_app();
        app.init_resource::<ProbeRequest>();
        app.add_systems(Update, probe.in_set(ShutdownSet::Command));
        app.update(); // Startup; drain install-time state.
        let _ = drain::<ServerBroadcast>(&app);

        // Both triggers arrive before a single update.
        app.world()
            .resource::<ShutdownSignal>()
            .0
            .store(true, Ordering::SeqCst);
        app.world_mut().resource_mut::<ProbeRequest>().0 = true;
        app.update();

        // Exactly one countdown scheduled, one warning broadcast — the probe saw
        // the poll's schedule through the chain and stood down.
        assert!(app.world().get_resource::<ActiveShutdown>().is_some());
        assert_eq!(drain::<ServerBroadcast>(&app).len(), 1);
    }
}
