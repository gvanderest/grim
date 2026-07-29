use std::collections::HashMap;

use bevy::prelude::Resource;

use crate::events::Command;

type CommandFactory = fn(&str) -> Option<Command>;

/// A trie-based command registry with "last registered wins" priority.
///
/// Registration order determines tiebreaking: when multiple command names
/// share a prefix, the most recently registered one takes priority.
///
/// # Lookup rules
///
/// Given input word W, finds every registered command whose name starts with W
/// (case-insensitive). Among those, returns the last-registered one.
///
/// This means:
/// - Typing a full name is an exact (prefix) match of itself
/// - Typing a prefix like "nor" matches both "north" and "nordic" if registered
/// - Last registration of those two wins
#[derive(Resource)]
pub struct CommandRegistry {
    root: TrieNode,
    entries: Vec<CommandEntry>,
}

struct TrieNode {
    children: HashMap<char, TrieNode>,
    /// Index into `entries` if this node is a terminal (end of a registered name).
    entry_idx: Option<usize>,
}

struct CommandEntry {
    _name: String,
    factory: CommandFactory,
}

impl CommandRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            root: TrieNode {
                children: HashMap::new(),
                entry_idx: None,
            },
            entries: Vec::new(),
        }
    }

    /// Register a command `name` with the given `factory`.
    ///
    /// The factory receives the "rest" of the input line (everything after the
    /// command word, trimmed) and returns `Some(Command)` or `None`.
    ///
    /// Later registrations win over earlier ones for overlapping prefixes.
    pub fn register(&mut self, name: &str, factory: CommandFactory) {
        let idx = self.entries.len();
        self.entries.push(CommandEntry {
            _name: name.to_string(),
            factory,
        });

        let mut node = &mut self.root;
        for c in name.chars() {
            node = node.children.entry(c).or_insert_with(|| TrieNode {
                children: HashMap::new(),
                entry_idx: None,
            });
        }
        node.entry_idx = Some(idx);
    }

    /// Resolve a command word to its `Command`, using the "rest" of the input.
    ///
    /// Matching is case-insensitive: the input `word` is lowercased before
    /// trie traversal.
    ///
    /// Returns `None` when no registered command name starts with `word`.
    pub fn resolve(&self, word: &str, rest: &str) -> Option<Command> {
        let word_lower: Vec<char> = word.chars().flat_map(|c| c.to_lowercase()).collect();
        let mut node = &self.root;

        for &c in &word_lower {
            let child = node.children.get(&c)?;
            node = child;
        }

        // Collect the last-registered command in the subtree (DFS, track max idx).
        let best_idx = self.collect_last_entry(node);
        best_idx.and_then(|idx| (self.entries[idx].factory)(rest))
    }

    /// Walk the trie subtree rooted at `node` and return the largest entry index found.
    fn collect_last_entry(&self, node: &TrieNode) -> Option<usize> {
        let mut best = node.entry_idx;
        for child in node.children.values() {
            if let Some(idx) = self.collect_last_entry(child) {
                best = Some(best.map_or(idx, |b| b.max(idx)));
            }
        }
        best
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::cardinal::Cardinal;
    use crate::events::Command;

    use super::*;

    #[test]
    fn test_exact_match() {
        let mut r = CommandRegistry::new();
        r.register("north", |_: &str| {
            Some(Command::Move {
                direction: Cardinal::North,
            })
        });
        assert_eq!(
            r.resolve("north", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
    }

    #[test]
    fn test_prefix_match() {
        let mut r = CommandRegistry::new();
        r.register("north", |_: &str| {
            Some(Command::Move {
                direction: Cardinal::North,
            })
        });
        assert_eq!(
            r.resolve("n", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            r.resolve("no", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            r.resolve("nor", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            r.resolve("nort", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
    }

    #[test]
    fn test_last_registered_wins() {
        let mut r = CommandRegistry::new();
        r.register("note", |rest| {
            Some(Command::Say {
                text: rest.to_string(),
            })
        });
        r.register("nordic", |rest| {
            Some(Command::Say {
                text: rest.to_string(),
            })
        });
        r.register("north", |_: &str| {
            Some(Command::Move {
                direction: Cardinal::North,
            })
        });

        assert_eq!(
            r.resolve("n", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            r.resolve("no", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            r.resolve("nor", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            r.resolve("nort", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            r.resolve("nord", ""),
            Some(Command::Say {
                text: "".to_string()
            })
        );
        assert_eq!(
            r.resolve("not", ""),
            Some(Command::Say {
                text: "".to_string()
            })
        );
    }

    #[test]
    fn test_no_match() {
        let mut r = CommandRegistry::new();
        r.register("north", |_: &str| {
            Some(Command::Move {
                direction: Cardinal::North,
            })
        });
        r.register("look", |rest| {
            if rest.is_empty() {
                Some(Command::Look { target: None })
            } else {
                Some(Command::Look {
                    target: Some(rest.to_string()),
                })
            }
        });

        assert_eq!(r.resolve("foobar", ""), None);
        assert_eq!(r.resolve("xyzzy", ""), None);
        assert_eq!(r.resolve("123", ""), None);
        assert_eq!(r.resolve("l", ""), Some(Command::Look { target: None }));
        assert_eq!(r.resolve("lo", ""), Some(Command::Look { target: None }));
        assert_eq!(
            r.resolve("lo", "statue"),
            Some(Command::Look {
                target: Some("statue".to_string())
            })
        );
    }

    #[test]
    fn test_empty_registry() {
        let r = CommandRegistry::new();
        assert_eq!(r.resolve("anything", ""), None);
    }

    #[test]
    fn test_shorthand_aliases() {
        let mut r = CommandRegistry::new();
        r.register("say", |rest| {
            if rest.is_empty() {
                None
            } else {
                Some(Command::Say {
                    text: rest.to_string(),
                })
            }
        });
        r.register("'", |rest| {
            if rest.is_empty() {
                None
            } else {
                Some(Command::Say {
                    text: rest.to_string(),
                })
            }
        });

        assert_eq!(
            r.resolve("say", "hello"),
            Some(Command::Say {
                text: "hello".to_string()
            })
        );
        assert_eq!(
            r.resolve("'", "hello"),
            Some(Command::Say {
                text: "hello".to_string()
            })
        );
        assert_eq!(r.resolve("say", ""), None);
        assert_eq!(r.resolve("'", ""), None);
    }
}
