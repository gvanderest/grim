//! Command resolution.
//!
//! A [`CommandRegistry`] maps the first word of a line to the command that
//! registered that name, resolving abbreviations by prefix. It is generic over
//! the produced command type `C`, so it carries no game vocabulary of its own —
//! a caller registers `fn(&str) -> Option<C>` factories and gets `C` back.
//!
//! # Resolution order
//!
//! 1. **Exact match** — a full command name always wins, even when a
//!    higher-priority command shares it as a prefix.
//! 2. **Prefix by priority** — among commands whose name starts with the typed
//!    word, the highest-priority one wins.
//!
//! # Priority
//!
//! Priority is an explicit ordering, not a number. Registering a command puts
//! it at the **front** (highest), so by default the most recently registered
//! command wins a contested prefix — a MUD author installs plugins and the last
//! one to claim `n` wins. To override, call [`CommandRegistry::prioritize`] or
//! [`CommandRegistry::deprioritize`], which move a command to the front or back
//! of the ordering. Nothing else shifts.
//!
//! Because a plugin claiming a prefix can silently change what an abbreviation
//! does, [`CommandRegistry::contested_prefixes`] reports every prefix more than
//! one command answers to, so the ambiguity can be logged at startup rather than
//! discovered by a confused player.

use bevy::prelude::Resource;

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

/// A prefix that more than one command answers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contest {
    /// The typed abbreviation, e.g. `"n"`.
    pub prefix: String,
    /// The command that currently wins it.
    pub winner: String,
    /// The commands it shadows, in priority order.
    pub shadowed: Vec<String>,
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

    /// Every prefix more than one command answers to, each with its current
    /// winner and the commands it shadows. A prefix that is itself an exact
    /// command name is not reported — an exact match always wins, so it is not
    /// contested. Results are sorted by prefix for stable logging.
    pub fn contested_prefixes(&self) -> Vec<Contest> {
        use std::collections::BTreeMap;

        let exact: Vec<&str> = self.entries.iter().map(|e| e.name.as_str()).collect();

        // prefix -> entry indices whose name starts with it
        let mut by_prefix: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, entry) in self.entries.iter().enumerate() {
            let chars: Vec<char> = entry.name.chars().collect();
            for len in 1..=chars.len() {
                let p: String = chars[..len].iter().collect();
                by_prefix.entry(p).or_default().push(i);
            }
        }

        by_prefix
            .into_iter()
            .filter(|(p, idxs)| idxs.len() >= 2 && !exact.contains(&p.as_str()))
            .map(|(prefix, idxs)| {
                // order the matching entries by priority
                let ordered: Vec<String> = self
                    .priority
                    .iter()
                    .copied()
                    .filter(|i| idxs.contains(i))
                    .map(|i| self.entries[i].name.clone())
                    .collect();
                let winner = ordered[0].clone();
                let shadowed = ordered[1..].to_vec();
                Contest {
                    prefix,
                    winner,
                    shadowed,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum Cmd {
        North,
        Note(String),
        Nordic(String),
        Look(Option<String>),
    }

    fn look(rest: &str) -> Option<Cmd> {
        Some(Cmd::Look((!rest.is_empty()).then(|| rest.to_string())))
    }

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

    #[test]
    fn contested_prefixes_reports_winner_and_shadowed() {
        let mut r = CommandRegistry::new();
        r.register("note", |rest| Some(Cmd::Note(rest.to_string())));
        r.register("nordic", |rest| Some(Cmd::Nordic(rest.to_string())));
        r.register("north", |_| Some(Cmd::North));

        let contests = r.contested_prefixes();
        // "n" is answered by all three; north (last) wins.
        let n = contests.iter().find(|c| c.prefix == "n").unwrap();
        assert_eq!(n.winner, "north");
        assert_eq!(n.shadowed, vec!["nordic".to_string(), "note".to_string()]);

        // "nor" is answered by nordic + north; north wins.
        let nor = contests.iter().find(|c| c.prefix == "nor").unwrap();
        assert_eq!(nor.winner, "north");
        assert_eq!(nor.shadowed, vec!["nordic".to_string()]);
    }

    #[test]
    fn exact_name_prefix_is_not_contested() {
        let mut r = CommandRegistry::new();
        // "no" is both an exact command and a prefix of "north".
        r.register("no", |_| Some(Cmd::North));
        r.register("north", |_| Some(Cmd::North));
        let contests = r.contested_prefixes();
        // "no" is exact -> not reported; "n" is still contested.
        assert!(contests.iter().all(|c| c.prefix != "no"));
        assert!(contests.iter().any(|c| c.prefix == "n"));
    }

    #[test]
    fn no_collisions_means_no_contests() {
        let mut r = CommandRegistry::new();
        r.register("look", look);
        r.register("quit", |_| Some(Cmd::North));
        assert!(r.contested_prefixes().is_empty());
    }
}
