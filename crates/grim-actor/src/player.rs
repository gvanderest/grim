//! Player-connection markers: [`Player`] (a character controlled by a live or
//! linkdead connection), [`Linkdead`] (disconnected but still in-world), and
//! [`OutputHistory`] (the recent-output ring buffer replayed on reconnect).

use std::collections::VecDeque;

use bevy::prelude::*;

/// Marks a character as player-controlled and links to their connection for output.
/// `connection: None` means the player is linkdead (disconnected but still in-world).
#[derive(Component, Debug)]
pub struct Player {
    /// The `Connection` entity to send output to, or `None` if linkdead.
    pub connection: Option<Entity>,
}

/// A bounded ring buffer of the lines most recently sent to a player, replayed
/// when they reconnect so a linkdead gap doesn't swallow context.
#[derive(Component, Debug, Default, Clone)]
pub struct OutputHistory {
    pub lines: VecDeque<String>,
    pub max: usize,
}

impl OutputHistory {
    pub fn with_max(max: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(max),
            max,
        }
    }
    pub fn push(&mut self, line: &str) {
        // A zero-capacity history retains nothing — bail before storing, else
        // the pop-then-push below would leave one line despite `max == 0`.
        if self.max == 0 {
            return;
        }
        if self.lines.len() >= self.max {
            self.lines.pop_front();
        }
        self.lines.push_back(line.to_string());
    }
    pub fn drain(&mut self) -> Vec<String> {
        self.lines.drain(..).collect()
    }
}

/// Character is still in-world but the player disconnected (linkdead).
#[derive(Component, Debug)]
pub struct Linkdead;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_history_evicts_oldest_past_max() {
        let mut h = OutputHistory::with_max(2);
        h.push("a");
        h.push("b");
        h.push("c");
        assert_eq!(h.lines.len(), 2);
        assert_eq!(h.drain(), vec!["b".to_string(), "c".to_string()]);
    }

    #[test]
    fn output_history_drain_empties() {
        let mut h = OutputHistory::with_max(4);
        h.push("x");
        assert_eq!(h.drain(), vec!["x".to_string()]);
        assert!(h.lines.is_empty());
        // A second drain yields nothing.
        assert!(h.drain().is_empty());
    }

    #[test]
    fn output_history_zero_capacity_retains_nothing() {
        let mut h = OutputHistory::with_max(0);
        h.push("a");
        h.push("b");
        assert!(h.lines.is_empty(), "max == 0 must retain nothing");
        assert!(h.drain().is_empty());
        // `default()` is also zero-capacity — same contract.
        let mut d = OutputHistory::default();
        d.push("x");
        assert!(d.lines.is_empty());
    }
}
