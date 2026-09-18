//! Global flood shed: the accept-rate gate for the listener.
//!
//! When accepts arrive faster than `max_connects_total` per
//! `total_window_secs`, the listener sheds (accept-and-immediately-close,
//! no handshake, no event) for `shed_secs`. All state is task-local to
//! the accept loop; nothing here crosses a copyover. Split out of
//! `server.rs` under the module line cap.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use crate::bridge::NetworkEvent;
use crate::limits::TelnetLimits;

/// Accept-rate state for the global flood shed. `accepts` is pruned to
/// the configured window on every accept, so it stays bounded by the
/// accept rate itself.
#[derive(Default)]
pub(crate) struct ShedState {
    pub(crate) accepts: VecDeque<Instant>,
    pub(crate) shed_until: Option<Instant>,
    pub(crate) last_report: Option<Instant>,
    pub(crate) refused: u64,
    /// Refused total of a just-lifted shed, carried to the accounting
    /// below so the lifting accept reports the exit exactly once.
    pub(crate) lifted_refused: Option<u64>,
}

/// What the accept gate decides for one fresh socket.
pub(crate) enum AcceptOutcome {
    /// Normal path: greet and register.
    Admit,
    /// Admit, and also forward a shed-transition event first.
    AdmitWith(NetworkEvent),
    /// Shed is active: drop the socket without a handshake or event.
    Drop,
    /// Drop, and also forward a periodic shed-heartbeat event.
    DropWith(NetworkEvent),
}

/// Shed-heartbeat interval: shed refusals are counted every accept but
/// reported at most this often — per-accept logging would be its own DoS.
const SHED_REPORT_INTERVAL: Duration = Duration::from_secs(60);

/// Decide one fresh accept against the global flood shed. Copyover
/// re-adoptions bypass this (they are adopted, not accepted).
pub(crate) fn gate_accept(limits: &TelnetLimits, shed: &Mutex<ShedState>) -> AcceptOutcome {
    let mut shed = shed.lock().unwrap();
    let now = Instant::now();
    let window = Duration::from_secs(limits.total_window_secs);

    if let Some(until) = shed.shed_until {
        if now < until {
            shed.refused += 1;
            let due = shed
                .last_report
                .is_none_or(|t| now.duration_since(t) >= SHED_REPORT_INTERVAL);
            if !due {
                return AcceptOutcome::Drop;
            }
            shed.last_report = Some(now);
            return AcceptOutcome::DropWith(shed_event(true, shed.refused, limits));
        }
        // Expired: lift the shed, then account this accept normally below
        // (it counts, and may re-trip immediately under an ongoing flood).
        let lifted_refused = shed.refused;
        shed.shed_until = None;
        shed.last_report = None;
        shed.refused = 0;
        shed.lifted_refused = Some(lifted_refused);
    }

    while shed
        .accepts
        .front()
        .is_some_and(|t| now.duration_since(*t) > window)
    {
        shed.accepts.pop_front();
    }
    shed.accepts.push_back(now);
    if shed.accepts.len() > limits.max_connects_total as usize {
        shed.shed_until = Some(now + Duration::from_secs(limits.shed_secs));
        shed.last_report = Some(now);
        shed.refused = 0;
        shed.lifted_refused = None;
        // The connection that trips the shed arrived before it started:
        // admit it, and report the entry.
        return AcceptOutcome::AdmitWith(shed_event(true, 0, limits));
    }
    if let Some(refused) = shed.lifted_refused.take() {
        return AcceptOutcome::AdmitWith(shed_event(false, refused, limits));
    }
    AcceptOutcome::Admit
}

/// Build a shed-transition/heartbeat event snapshotting the current knobs.
fn shed_event(active: bool, refused: u64, limits: &TelnetLimits) -> NetworkEvent {
    NetworkEvent::Shed {
        active,
        connects: limits.max_connects_total,
        window_secs: limits.total_window_secs,
        shed_secs: limits.shed_secs,
        refused,
    }
}
