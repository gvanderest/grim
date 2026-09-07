//! Raw `<target>` text → [`TargetSpec`]: which selector prefix (if any) plus
//! the lowercased keyword terms to match.
//!
//! Syntax (each piece gated by [`ParseOptions`]):
//!
//! - `sword` / `"brass lantern"` — terms split on whitespace; double quotes
//!   group words so `give`/`steal` can tell the item phrase from the being.
//! - `2.sword`, `2."brass lantern"` — offset: the 2nd best match (1-indexed).
//! - `3*sword`, `3*"brass lantern"` — quantity: up to 3 best matches.
//! - `all`, `all sword`, `all.sword` — every match (bare `all` needs no terms).
//!
//! Quantity and offset never combine (`10*2.sword` is invalid); a disallowed
//! prefix stays literal text (`look all` searches for something named "all").
//! `0*sword` / `0.sword` are invalid (there is no zeroth match).

use std::num::NonZeroUsize;

/// Which selector mechanisms a command accepts. Item verbs (`get`/`drop` and
/// the item side of `give`/`steal`) take [`ParseOptions::ITEM`]; being targets
/// (`look`, the being side) take [`ParseOptions::BEING`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOptions {
    /// `N*terms` — take up to N matches.
    pub allow_quantity: bool,
    /// `N.terms` — take the Nth match.
    pub allow_offset: bool,
    /// `all [terms]` — take every match.
    pub allow_all: bool,
}

impl ParseOptions {
    /// Item targets: quantity, offset, and `all` all apply.
    pub const ITEM: Self = Self {
        allow_quantity: true,
        allow_offset: true,
        allow_all: true,
    };
    /// Being targets: only the offset applies (one being is never a quantity).
    pub const BEING: Self = Self {
        allow_quantity: false,
        allow_offset: true,
        allow_all: false,
    };
}

/// How many of the ranked matches a [`TargetSpec`] wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selector {
    /// The single best match.
    One,
    /// The Nth best match (1-indexed): `2.sword` is the second sword.
    Nth(NonZeroUsize),
    /// Every match, best first: `all`, `all sword`.
    All,
    /// Up to N best matches: `3*sword` takes three.
    Many(NonZeroUsize),
}

/// A parsed target: lowercased keyword terms plus which matches to take.
/// Terms combine with AND — every term must prefix-match for a candidate to
/// rank (see [`crate::rank_match`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSpec {
    /// Lowercased terms. Empty only for a bare `all`.
    pub terms: Vec<String>,
    pub selector: Selector,
}

/// Parse `raw` under `options`. `None` means invalid: empty input, an empty
/// term list (`""`), an unterminated quote, a zero count, or combined
/// quantity+offset prefixes.
pub fn parse_target(raw: &str, options: ParseOptions) -> Option<TargetSpec> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if options.allow_all && is_all_form(trimmed) {
        let rest = trimmed[3..]
            .strip_prefix(|c| c == '.' || c == '*')
            .unwrap_or(&trimmed[3..]);
        let rest = rest.trim_start();
        if rest.is_empty() {
            return Some(TargetSpec {
                terms: Vec::new(),
                selector: Selector::All,
            });
        }
        let terms = split_target_terms(rest)?;
        return Some(TargetSpec {
            terms,
            selector: Selector::All,
        });
    }
    if options.allow_quantity {
        if let Some((count, rest)) = split_count_prefix(trimmed, '*') {
            if is_offset_form(rest) {
                return None;
            }
            let terms = split_target_terms(rest)?;
            return Some(TargetSpec {
                terms,
                selector: Selector::Many(NonZeroUsize::new(count)?),
            });
        }
    }
    if options.allow_offset {
        if let Some((nth, rest)) = split_count_prefix(trimmed, '.') {
            if is_quantity_form(rest) {
                return None;
            }
            let terms = split_target_terms(rest)?;
            return Some(TargetSpec {
                terms,
                selector: Selector::Nth(NonZeroUsize::new(nth)?),
            });
        }
    }
    let terms = split_target_terms(trimmed)?;
    Some(TargetSpec {
        terms,
        selector: Selector::One,
    })
}

/// `all`, case-insensitive, followed by a boundary (end, space, `.`, `*`, or
/// `"`) so `alloy` stays a plain word.
fn is_all_form(trimmed: &str) -> bool {
    // `get(..3)`: a leading multibyte character is never `all` — and byte
    // slicing would panic on it (`give épée bob` must miss, not crash).
    let Some(head) = trimmed.get(..3) else {
        return false;
    };
    if !head.eq_ignore_ascii_case("all") {
        return false;
    }
    match trimmed.as_bytes().get(3) {
        None => true,
        Some(b' ' | b'\t' | b'.' | b'*' | b'"') => true,
        Some(_) => false,
    }
}

/// Leading `<digits><delim>`: `(10, "sword")` from `"10*sword"`. `None` when
fn split_count_prefix(s: &str, delim: char) -> Option<(usize, &str)> {
    let digits = s.len() - s.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return None;
    }
    let rest = &s[digits..];
    let after = rest.strip_prefix(delim)?;
    Some((s[..digits].parse::<usize>().ok()?, after))
}

fn is_offset_form(s: &str) -> bool {
    split_count_prefix(s.trim_start(), '.').is_some()
}

fn is_quantity_form(s: &str) -> bool {
    split_count_prefix(s.trim_start(), '*').is_some()
}

/// Split `raw` into lowercased terms on whitespace. `"` quotes are stripped
/// first: quoted and unquoted multi-word targets parse identically (`"brass
/// lantern"` and `brass lantern` both mean every term must match). Quoting
/// matters one layer up, where `give`/`steal` split `<item> <target>` — a
/// quoted phrase stays on one side of that divide. `None` when no terms
/// remain, or when the quotes do not balance (an unterminated quote is
/// malformed, matching the `give`/`steal` split — fail closed, never guess).
pub fn split_target_terms(raw: &str) -> Option<Vec<String>> {
    if raw.chars().filter(|c| *c == '"').count() % 2 != 0 {
        return None;
    }
    let terms: Vec<String> = raw
        .replace('"', "")
        .split_whitespace()
        .map(|word| word.to_lowercase())
        .collect();
    (!terms.is_empty()).then_some(terms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(term: &str) -> Option<TargetSpec> {
        Some(TargetSpec {
            terms: vec![term.into()],
            selector: Selector::One,
        })
    }

    #[test]
    fn empty_or_blank_is_invalid() {
        assert_eq!(parse_target("", ParseOptions::ITEM), None);
        assert_eq!(parse_target("   ", ParseOptions::ITEM), None);
        assert_eq!(parse_target("", ParseOptions::BEING), None);
    }

    #[test]
    fn plain_word_is_one_term() {
        assert_eq!(parse_target("sword", ParseOptions::ITEM), one("sword"));
        assert_eq!(parse_target("  Sword  ", ParseOptions::ITEM), one("sword"));
    }

    #[test]
    fn quoted_group_splits_into_terms() {
        assert_eq!(
            parse_target("\"brass lantern\"", ParseOptions::ITEM),
            Some(TargetSpec {
                terms: vec!["brass".into(), "lantern".into()],
                selector: Selector::One,
            })
        );
        assert_eq!(
            parse_target("brass lantern", ParseOptions::ITEM),
            parse_target("\"brass lantern\"", ParseOptions::ITEM)
        );
    }

    #[test]
    fn unterminated_quote_is_rejected() {
        // Malformed quotes fail closed, matching the `give`/`steal` split:
        // handlers answer with their miss line instead of guessing.
        assert_eq!(parse_target("\"brass", ParseOptions::ITEM), None);
        assert_eq!(parse_target("2.\"brass", ParseOptions::BEING), None);
        assert_eq!(parse_target("brass\"", ParseOptions::ITEM), None);
    }

    #[test]
    fn lone_quotes_are_invalid() {
        assert_eq!(parse_target("\"\"", ParseOptions::ITEM), None);
    }

    #[test]
    fn offset_selects_nth() {
        let spec = parse_target("2.sword", ParseOptions::ITEM).unwrap();
        assert_eq!(spec.terms, vec!["sword".to_string()]);
        assert_eq!(spec.selector, Selector::Nth(NonZeroUsize::new(2).unwrap()));
    }

    #[test]
    fn offset_with_quoted_group() {
        let spec = parse_target("2.\"pot health\"", ParseOptions::ITEM).unwrap();
        assert_eq!(spec.terms, vec!["pot".to_string(), "health".to_string()]);
        assert_eq!(spec.selector, Selector::Nth(NonZeroUsize::new(2).unwrap()));
    }

    #[test]
    fn offset_disallowed_stays_literal() {
        // `look` allows offsets, so use a no-offset option set instead.
        let none = ParseOptions {
            allow_quantity: false,
            allow_offset: false,
            allow_all: false,
        };
        assert_eq!(
            parse_target("2.sword", none),
            Some(TargetSpec {
                terms: vec!["2.sword".into()],
                selector: Selector::One,
            })
        );
    }

    #[test]
    fn quantity_selects_many() {
        let spec = parse_target("10*potion", ParseOptions::ITEM).unwrap();
        assert_eq!(spec.terms, vec!["potion".to_string()]);
        assert_eq!(
            spec.selector,
            Selector::Many(NonZeroUsize::new(10).unwrap())
        );
    }

    #[test]
    fn quantity_with_quoted_group() {
        let spec = parse_target("10*\"potion health\"", ParseOptions::ITEM).unwrap();
        assert_eq!(spec.terms, vec!["potion".to_string(), "health".to_string()]);
    }

    #[test]
    fn quantity_disallowed_stays_literal() {
        assert_eq!(
            parse_target("10*potion", ParseOptions::BEING),
            Some(TargetSpec {
                terms: vec!["10*potion".into()],
                selector: Selector::One,
            })
        );
    }

    #[test]
    fn zero_counts_are_invalid() {
        assert_eq!(parse_target("0*sword", ParseOptions::ITEM), None);
        assert_eq!(parse_target("0.sword", ParseOptions::ITEM), None);
    }

    #[test]
    fn combined_prefixes_are_invalid() {
        assert_eq!(parse_target("10*2.sword", ParseOptions::ITEM), None);
        assert_eq!(parse_target("2.10*sword", ParseOptions::ITEM), None);
    }

    #[test]
    fn missing_terms_are_invalid() {
        assert_eq!(parse_target("10*", ParseOptions::ITEM), None);
        assert_eq!(parse_target("2.", ParseOptions::ITEM), None);
        assert_eq!(parse_target("2.  ", ParseOptions::ITEM), None);
    }

    #[test]
    fn bare_all_selects_everything() {
        assert_eq!(
            parse_target("all", ParseOptions::ITEM),
            Some(TargetSpec {
                terms: Vec::new(),
                selector: Selector::All,
            })
        );
        assert_eq!(
            parse_target("  ALL  ", ParseOptions::ITEM),
            parse_target("all", ParseOptions::ITEM)
        );
    }

    #[test]
    fn all_with_terms() {
        assert_eq!(
            parse_target("all sword", ParseOptions::ITEM),
            Some(TargetSpec {
                terms: vec!["sword".into()],
                selector: Selector::All,
            })
        );
        assert_eq!(
            parse_target("all.sword", ParseOptions::ITEM),
            parse_target("all sword", ParseOptions::ITEM)
        );
        assert_eq!(
            parse_target("all \"brass lantern\"", ParseOptions::ITEM),
            Some(TargetSpec {
                terms: vec!["brass".into(), "lantern".into()],
                selector: Selector::All,
            })
        );
    }

    #[test]
    fn all_disallowed_stays_literal() {
        assert_eq!(parse_target("all", ParseOptions::BEING), one("all"));
    }

    #[test]
    fn all_prefix_needs_a_boundary() {
        assert_eq!(parse_target("alloy", ParseOptions::ITEM), one("alloy"));
        assert_eq!(
            parse_target("allure potion", ParseOptions::ITEM),
            Some(TargetSpec {
                terms: vec!["allure".into(), "potion".into()],
                selector: Selector::One,
            })
        );
    }

    #[test]
    fn multibyte_input_stays_literal() {
        // Byte slicing must never panic: a leading multibyte character is
        // not `all`, just terms.
        assert_eq!(
            parse_target("épée bob", ParseOptions::ITEM),
            Some(TargetSpec {
                terms: vec!["épée".into(), "bob".into()],
                selector: Selector::One,
            })
        );
    }
}
