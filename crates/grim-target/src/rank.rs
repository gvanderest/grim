//! Keyword ranking: one candidate name (+ keywords) against parsed terms.
//!
//! A single term keeps the legacy `look` semantics exactly: exact full name,
//! then exact keyword, then full-name/keyword prefix. Multiple terms combine
//! with AND — every term must prefix-match a word of the name or a keyword —
//! and always rank at the prefix tier, so `2."pot health"` finds the second
//! health potion. Lower tiers sort first; ties break by shortest name, then
//! alphabetically (the [`Rank`] field order is the sort order).

/// Sortable match quality: field order is the tiebreak order (tier, then
/// shortest name, then alphabetical).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rank {
    /// 0 = exact name, 1 = exact keyword, 2 = prefix.
    pub tier: u8,
    /// Full-name length (prefix tier only; shorter names win).
    pub name_len: usize,
    /// Lowercased full name (prefix tier only; alphabetical).
    pub name: String,
}

/// Rank `name` (+ `keywords`) against already-lowercased `terms`. `None`
/// when the candidate does not match, or when `terms` is empty (a bare `all`
/// bypasses ranking — see [`crate::query`]).
pub fn rank_match(name: &str, keywords: &[String], terms: &[String]) -> Option<Rank> {
    if terms.is_empty() {
        return None;
    }
    if terms.len() == 1 {
        return rank_single(name, keywords, &terms[0]);
    }
    rank_group(name, keywords, terms)
}

/// The legacy single-term ranking: exact name (0), exact keyword (1), or
/// full-name/keyword prefix (2, shortest name then alphabetical).
fn rank_single(name: &str, keywords: &[String], want: &str) -> Option<Rank> {
    let lower = name.to_lowercase();
    if lower == *want {
        return Some(Rank {
            tier: 0,
            name_len: 0,
            name: String::new(),
        });
    }
    if keywords.iter().any(|k| k.to_lowercase() == *want) {
        return Some(Rank {
            tier: 1,
            name_len: 0,
            name: String::new(),
        });
    }
    let prefix =
        lower.starts_with(want) || keywords.iter().any(|k| k.to_lowercase().starts_with(want));
    prefix.then_some(Rank {
        tier: 2,
        name_len: name.len(),
        name: lower,
    })
}

/// Multi-term AND: every term must prefix-match a word of the name or a
/// keyword. Always the prefix tier — a group is inherently inexact.
fn rank_group(name: &str, keywords: &[String], terms: &[String]) -> Option<Rank> {
    let lower = name.to_lowercase();
    let lowered: Vec<String> = keywords.iter().map(|k| k.to_lowercase()).collect();
    let mut tokens: Vec<&str> = lower.split_whitespace().collect();
    for keyword in &lowered {
        tokens.extend(keyword.split_whitespace());
    }
    let all = terms
        .iter()
        .all(|term| tokens.iter().any(|token| token.starts_with(term)));
    all.then_some(Rank {
        tier: 2,
        name_len: name.len(),
        name: lower,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kw(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn empty_terms_never_match() {
        assert_eq!(rank_match("sword", &kw(&["sword"]), &[]), None);
    }

    #[test]
    fn exact_name_beats_everything() {
        let exact = rank_match("wrack", &[], &["wrack".into()]).unwrap();
        let prefix = rank_match("wrackus", &[], &["wrack".into()]).unwrap();
        assert!(exact < prefix);
        assert_eq!(exact.tier, 0);
    }

    #[test]
    fn exact_keyword_beats_name_prefix() {
        let by_keyword =
            rank_match("Grimmok Ironhand", &kw(&["smith"]), &["smith".into()]).unwrap();
        let by_prefix = rank_match("Smithson", &[], &["smith".into()]).unwrap();
        assert_eq!(by_keyword.tier, 1);
        assert_eq!(by_prefix.tier, 2);
        assert!(by_keyword < by_prefix);
    }

    #[test]
    fn keyword_prefix_matches_case_insensitively() {
        // Terms arrive lowercased from `parse_target`; names and keywords may
        // use any case.
        assert!(rank_match("goblin", &kw(&["GOB"]), &["gob".into()]).is_some());
        assert!(rank_match("GOBLIN", &[], &["gob".into()]).is_some());
    }

    #[test]
    fn shortest_prefix_name_wins() {
        let short = rank_match("Wrack", &[], &["wrack".into()]).unwrap();
        let long = rank_match("Wrackus", &[], &["wrack".into()]).unwrap();
        assert!(short < long);
    }

    #[test]
    fn non_matching_term_is_none() {
        assert_eq!(
            rank_match("sword", &kw(&["blade"]), &["potion".into()]),
            None
        );
    }

    #[test]
    fn group_requires_every_term() {
        let keywords = kw(&["potion", "health"]);
        let both = ["pot".to_string(), "health".to_string()];
        assert!(rank_match("health potion", &keywords, &both).is_some());
        let one_missing = ["pot".to_string(), "mana".to_string()];
        assert!(rank_match("health potion", &keywords, &one_missing).is_none());
    }

    #[test]
    fn group_matches_name_words_without_keywords() {
        let both = ["brass".to_string(), "lantern".to_string()];
        let rank = rank_match("brass lantern", &[], &both).unwrap();
        assert_eq!(rank.tier, 2);
    }

    #[test]
    fn single_term_keeps_legacy_inner_word_miss() {
        // Legacy quirk, preserved: one term prefixes the full name or a
        // keyword, never an inner name word without keyword help.
        assert_eq!(rank_match("brass lantern", &[], &["lantern".into()]), None);
        assert!(rank_match("brass lantern", &kw(&["lantern"]), &["lantern".into()]).is_some());
    }
}
