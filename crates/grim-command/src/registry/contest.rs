//! Contested-prefix reporting: which abbreviations more than one command
//! answers to, and who wins each.

use super::CommandRegistry;

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
mod contest_tests {
    use super::super::fixture::{look, Cmd};
    use super::*;

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
