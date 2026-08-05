//! The registry itself: registration, priority ordering, and exact-then-prefix
//! resolution.

use bevy::prelude::Resource;

mod contest;

pub use contest::Contest;

/// A registry of named command factories with explicit priority.
///
/// Usable as a Bevy resource once instantiated with a concrete `C`
/// (`CommandRegistry<MyCommand>`).
#[derive(Resource)]
pub struct CommandRegistry<C: Send + Sync + 'static> {
    /// Entries in registration order; indices are stable once assigned.
    entries: Vec<Entry<C>>,
    /// Indices into `entries`, ordered most-preferred first.
    priority: Vec<usize>,
}

struct Entry<C> {
    name: String,
    factory: fn(&str) -> Option<C>,
}

impl<C: Send + Sync + 'static> Default for CommandRegistry<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Send + Sync + 'static> CommandRegistry<C> {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            priority: Vec::new(),
        }
    }

    /// Register `name` with a factory. The factory receives the rest of the
    /// line (everything after the command word, trimmed) and returns the
    /// command, or `None` to reject the input (e.g. `say` with no text).
    ///
    /// The new command starts at the front of the priority ordering.
    pub fn register(&mut self, name: &str, factory: fn(&str) -> Option<C>) {
        let idx = self.entries.len();
        self.entries.push(Entry {
            name: name.to_ascii_lowercase(),
            factory,
        });
        self.priority.insert(0, idx);
    }

    /// Resolve a command `word` (case-insensitive) to a command, passing `rest`
    /// to the matched factory. Returns `None` when nothing matches, or when the
    /// matched factory rejects `rest`.
    pub fn resolve(&self, word: &str, rest: &str) -> Option<C> {
        let word = word.to_ascii_lowercase();

        // 1. exact match wins outright, highest priority among exacts.
        if let Some(idx) = self
            .priority
            .iter()
            .copied()
            .find(|&i| self.entries[i].name == word)
        {
            return (self.entries[idx].factory)(rest);
        }

        // 2. otherwise the highest-priority prefix match.
        let idx = self
            .priority
            .iter()
            .copied()
            .find(|&i| self.entries[i].name.starts_with(&word))?;
        (self.entries[idx].factory)(rest)
    }

    /// Move a command to the front of the priority ordering (highest). No-op if
    /// the name is not registered.
    pub fn prioritize(&mut self, name: &str) {
        self.reorder(name, true);
    }

    /// Move a command to the back of the priority ordering (lowest). No-op if
    /// the name is not registered.
    pub fn deprioritize(&mut self, name: &str) {
        self.reorder(name, false);
    }

    fn reorder(&mut self, name: &str, front: bool) {
        let name = name.to_ascii_lowercase();
        let Some(idx) = self.entries.iter().position(|e| e.name == name) else {
            return;
        };
        self.priority.retain(|&i| i != idx);
        if front {
            self.priority.insert(0, idx);
        } else {
            self.priority.push(idx);
        }
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    //! Shared test command type + factory, reused by the resolution, reorder,
    //! and contest test submodules.

    #[derive(Debug, PartialEq, Eq)]
    pub(crate) enum Cmd {
        North,
        Note(String),
        Nordic(String),
        Look(Option<String>),
    }

    pub(crate) fn look(rest: &str) -> Option<Cmd> {
        Some(Cmd::Look((!rest.is_empty()).then(|| rest.to_string())))
    }
}

#[cfg(test)]
mod resolution_tests {
    use super::fixture::{look, Cmd};
    use super::*;

    #[test]
    fn exact_match_beats_higher_priority_prefix() {
        let mut r = CommandRegistry::new();
        r.register("look", look);
        // "l" registered later => higher priority, but "look" is an exact hit.
        r.register("l", look);
        assert_eq!(r.resolve("look", ""), Some(Cmd::Look(None)));
        // "l" is exact for the shorthand.
        assert_eq!(r.resolve("l", ""), Some(Cmd::Look(None)));
    }

    #[test]
    fn last_registered_wins_a_prefix_by_default() {
        let mut r = CommandRegistry::new();
        r.register("note", |rest| Some(Cmd::Note(rest.to_string())));
        r.register("nordic", |rest| Some(Cmd::Nordic(rest.to_string())));
        r.register("north", |_| Some(Cmd::North));
        // all three share "n"; north registered last => wins
        assert_eq!(r.resolve("n", ""), Some(Cmd::North));
        assert_eq!(r.resolve("no", ""), Some(Cmd::North));
        assert_eq!(r.resolve("nor", ""), Some(Cmd::North));
        // unambiguous longer prefixes still reach their own command
        assert_eq!(r.resolve("nord", ""), Some(Cmd::Nordic(String::new())));
        assert_eq!(r.resolve("not", ""), Some(Cmd::Note(String::new())));
    }

    #[test]
    fn resolve_is_case_insensitive() {
        let mut r = CommandRegistry::new();
        r.register("north", |_| Some(Cmd::North));
        assert_eq!(r.resolve("NORTH", ""), Some(Cmd::North));
        assert_eq!(r.resolve("N", ""), Some(Cmd::North));
    }

    #[test]
    fn factory_rejection_returns_none() {
        let mut r = CommandRegistry::new();
        r.register("say", |rest| {
            (!rest.is_empty()).then(|| Cmd::Note(rest.to_string()))
        });
        assert_eq!(r.resolve("say", ""), None);
        assert_eq!(r.resolve("say", "hi"), Some(Cmd::Note("hi".into())));
    }

    #[test]
    fn unknown_word_resolves_to_none() {
        let mut r = CommandRegistry::new();
        r.register("north", |_| Some(Cmd::North));
        assert_eq!(r.resolve("xyzzy", ""), None);
    }
}

#[cfg(test)]
mod reorder_tests {
    use super::fixture::Cmd;
    use super::*;

    #[test]
    fn prioritize_moves_to_front() {
        let mut r = CommandRegistry::new();
        r.register("note", |rest| Some(Cmd::Note(rest.to_string())));
        r.register("north", |_| Some(Cmd::North));
        assert_eq!(r.resolve("n", ""), Some(Cmd::North)); // north last => wins
        r.prioritize("note");
        assert_eq!(r.resolve("n", ""), Some(Cmd::Note(String::new())));
    }

    #[test]
    fn deprioritize_moves_to_back() {
        let mut r = CommandRegistry::new();
        r.register("note", |rest| Some(Cmd::Note(rest.to_string())));
        r.register("north", |_| Some(Cmd::North));
        r.deprioritize("north");
        assert_eq!(r.resolve("n", ""), Some(Cmd::Note(String::new())));
    }

    #[test]
    fn reorder_unknown_name_is_noop() {
        let mut r = CommandRegistry::new();
        r.register("north", |_| Some(Cmd::North));
        r.prioritize("nope");
        r.deprioritize("nope");
        assert_eq!(r.resolve("n", ""), Some(Cmd::North));
    }
}
