//! Token splitting for two-part verbs (`give` / `steal` item/target).
//!
//! Split from `parser` (file cap): shell-like token ranges plus the
//! `all`-head detector. Pure functions, unit-tested through the registry.

/// Split `<item> <target>` for `give`/`steal`. Tokens are shell-like: an
/// optional `N*`/`N.` selector prefix plus a bare word or a `"quoted phrase"`.
/// Usually the item is the first token and the being everything after it
/// (`give "brass lantern" bob`), but an `all`-headed item (`all`, `all sword`,
/// `all.sword`) is greedy — it runs to the last token so the being stays one
/// word (`give all sword "Grimmok Ironhand"`, quoted when multi-word). Both
/// parts required; an unterminated quote is unknown.
pub(crate) fn split_transfer(rest: &str) -> Option<(String, String)> {
    let rest = rest.trim();
    let tokens = split_tokens(rest)?;
    if tokens.is_empty() {
        return None;
    }
    if is_all_head(&rest[tokens[0].0..tokens[0].1]) {
        if tokens.len() < 2 {
            return None;
        }
        let (item_end, target_start) = (tokens[tokens.len() - 2].1, tokens[tokens.len() - 1].0);
        return Some((
            rest[..item_end].trim().to_string(),
            rest[target_start..].trim().to_string(),
        ));
    }
    let (first_start, first_end) = tokens[0];
    let target = rest[first_end..].trim();
    if target.is_empty() {
        return None;
    }
    Some((rest[first_start..first_end].to_string(), target.to_string()))
}

/// Byte ranges of shell-like tokens in `rest`: `[N*|N.]?(bare|"quoted")`.
/// `None` on an unterminated quote.
pub(crate) fn split_tokens(rest: &str) -> Option<Vec<(usize, usize)>> {
    let bytes = rest.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        // Optional `N*` / `N.` selector prefix stays glued to its token.
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j > i && j < bytes.len() && (bytes[j] == b'*' || bytes[j] == b'.') {
            i = j + 1;
        }
        if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            if i >= bytes.len() {
                return None;
            }
            i += 1;
            // Text glued to the closing quote (`"sword"bob`) is malformed:
            // without a delimiter the item/being divide is a guess.
            if i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                return None;
            }
        } else {
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
        }
        tokens.push((start, i));
    }
    Some(tokens)
}

/// Whether a split token heads an `all` item: `all` plus a boundary (end,
/// `.`, `*`, or `"`) so `alloy` stays a plain word. Case-insensitive.
pub(crate) fn is_all_head(token: &str) -> bool {
    // Strip a glued `N*` / `N.` prefix first (`2.all` still heads an all).
    let mut head = token;
    if let Some(digits) = head.find(|c: char| !c.is_ascii_digit()) {
        if digits > 0 {
            let after = &head[digits..];
            if let Some(stripped) = after.strip_prefix(|c| c == '*' || c == '.') {
                head = stripped;
            }
        }
    }
    let Some(head3) = head.get(..3) else {
        return false;
    };
    if !head3.eq_ignore_ascii_case("all") {
        return false;
    }
    match head.as_bytes().get(3) {
        None => true,
        Some(b'.' | b'*' | b'"') => true,
        Some(_) => false,
    }
}
