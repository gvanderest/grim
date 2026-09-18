//! Per-IP reconnect throttle: sliding-window attempts plus a hard-reject
//! window, enforced at the greeter gate alongside the IP ban check.
//!
//! The clock is Bevy `Time` seconds passed in by the caller, so tests
//! drive it with plain `now` values — no sleeps, no flaky windows
//! (AGENTS.md rule 11).

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;

use bevy::prelude::*;

/// Reconnect-throttle knobs. All time values are whole seconds.
#[derive(Resource, Clone, Debug)]
pub struct ReconnectLimits {
    /// Most new connections per IP inside `window_secs` before refusals.
    pub max_attempts: u32,
    /// Sliding window for `max_attempts`, in seconds.
    pub window_secs: u64,
    /// How long a tripped IP is refused outright, in seconds.
    pub reject_secs: u64,
}

impl Default for ReconnectLimits {
    fn default() -> Self {
        Self {
            max_attempts: 10,
            window_secs: 60,
            reject_secs: 300,
        }
    }
}

/// Recent connects per IP. Attempt timestamps prune to the window on
/// every check; see [`ReconnectThrottle::sweep`] for the memory bound.
#[derive(Resource, Debug, Default)]
pub(crate) struct ReconnectThrottle {
    attempts: HashMap<IpAddr, VecDeque<f64>>,
    rejected_until: HashMap<IpAddr, f64>,
}

/// Cap on tracked IPs; past it a full sweep evicts everything outside
/// the longest horizon. Bounds memory under slow IP rotation.
const MAX_TRACKED_IPS: usize = 8192;

impl ReconnectThrottle {
    /// Record one attempt from `ip` at `now` (seconds). Returns true
    /// when refused: inside a reject window, or past `max_attempts` in
    /// the sliding window (which arms a fresh reject window).
    pub(crate) fn check(&mut self, ip: &IpAddr, now: f64, limits: &ReconnectLimits) -> bool {
        if self
            .rejected_until
            .get(ip)
            .is_some_and(|&until| now < until)
        {
            return true;
        }
        let window = limits.window_secs as f64;
        let attempts = self.attempts.entry(*ip).or_default();
        while attempts.front().is_some_and(|&t| now - t > window) {
            attempts.pop_front();
        }
        attempts.push_back(now);
        if attempts.len() > limits.max_attempts as usize {
            self.rejected_until
                .insert(*ip, now + limits.reject_secs as f64);
            self.sweep(now, window.max(limits.reject_secs as f64));
            return true;
        }
        if self.attempts.len() > MAX_TRACKED_IPS {
            self.sweep(now, window.max(limits.reject_secs as f64));
        }
        false
    }

    /// Drop IPs with nothing inside `horizon` and no live reject window.
    fn sweep(&mut self, now: f64, horizon: f64) {
        self.rejected_until.retain(|_, &mut until| now < until);
        self.attempts.retain(|ip, times| {
            times.iter().any(|&t| now - t <= horizon) || self.rejected_until.contains_key(ip)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> ReconnectLimits {
        ReconnectLimits {
            max_attempts: 2,
            window_secs: 60,
            reject_secs: 300,
        }
    }

    fn ip() -> IpAddr {
        "10.9.9.9".parse().unwrap()
    }

    #[test]
    fn third_rapid_attempt_trips_and_arms_reject() {
        let mut throttle = ReconnectThrottle::default();
        assert!(!throttle.check(&ip(), 0.0, &limits()));
        assert!(!throttle.check(&ip(), 1.0, &limits()));
        assert!(throttle.check(&ip(), 2.0, &limits()));
        // Inside the reject window: refused without recording.
        assert!(throttle.check(&ip(), 100.0, &limits()));
        // Past it: attempts expired out of the window, admitted again.
        assert!(!throttle.check(&ip(), 400.0, &limits()));
    }

    #[test]
    fn other_ips_unaffected() {
        let mut throttle = ReconnectThrottle::default();
        let other: IpAddr = "10.9.9.10".parse().unwrap();
        assert!(!throttle.check(&ip(), 0.0, &limits()));
        assert!(!throttle.check(&ip(), 1.0, &limits()));
        assert!(throttle.check(&ip(), 2.0, &limits()));
        assert!(!throttle.check(&other, 3.0, &limits()));
    }

    #[test]
    fn sweep_bounds_tracked_ips() {
        let mut throttle = ReconnectThrottle::default();
        // Ancient attempts outside every horizon vanish on sweep.
        throttle.attempts.insert(ip(), VecDeque::from([0.0]));
        throttle.sweep(10000.0, 60.0);
        assert!(throttle.attempts.is_empty());
    }
}
