//! Colour-aware width: measuring and truncating markup by *visible* columns.
//!
//! Column layouts (the WHO grid) must align by what the player sees, not by raw
//! byte/char count — a `{R…{x` run occupies zero visible columns, and truncating
//! must never split a `{X` / `@xRGB` token (a half-token would render as garbage
//! or leak colour). These helpers walk the same `{`- and `@`-family grammar that
//! [`crate::ansi`] consumes, so they stay in agreement with what actually renders.

use std::iter::Peekable;
use std::str::Chars;

/// Whether `c` (the char after a `{`) is a recognised 16-colour/reset code —
/// i.e. `{c` renders to zero visible columns. Mirrors [`crate::ansi`]'s `{` arm.
fn is_brace_code(c: char) -> bool {
    matches!(
        c,
        'x' | 'X' | '9' | '1'
            ..='8'
                | 'k'
                | 'r'
                | 'g'
                | 'y'
                | 'b'
                | 'm'
                | 'c'
                | 'w'
                | 'K'
                | 'R'
                | 'G'
                | 'Y'
                | 'B'
                | 'M'
                | 'C'
                | 'W'
                | '!'
                | '@'
                | '#'
                | '%'
                | '^'
                | '&'
                | '*'
    )
}

/// Consume one markup unit from `cs`, returning its source text and how many
/// *visible* columns it occupies: 0 for a colour/reset code, 1 for a literal
/// char or an escaped `{{`/`@@`, and the literal length for an unknown/incomplete
/// code that renders verbatim. Matches [`crate::ansi`]'s consumption exactly.
fn next_unit(cs: &mut Peekable<Chars>) -> Option<(String, usize)> {
    let ch = cs.next()?;
    match ch {
        '{' => match cs.next() {
            None => Some(("{".to_string(), 1)),
            Some('{') => Some(("{{".to_string(), 1)), // renders to a single '{'
            Some(c) if is_brace_code(c) => Some((format!("{{{c}"), 0)),
            Some(c) => Some((format!("{{{c}"), 2)), // unknown → literal '{' + c
        },
        '@' => match cs.next() {
            None => Some(("@".to_string(), 1)),
            Some('@') => Some(("@@".to_string(), 1)),
            Some(c @ ('r' | 'R')) => Some((format!("@{c}"), 0)),
            Some(c @ ('x' | 'X' | 'b' | 'B')) => {
                let mut src = format!("@{c}");
                let mut hex = 0;
                while hex < 3 {
                    match cs.peek() {
                        Some(d) if d.is_ascii_hexdigit() => {
                            src.push(*d);
                            cs.next();
                            hex += 1;
                        }
                        _ => break,
                    }
                }
                // Exactly three hex → a colour token (0 visible); otherwise the
                // `@x`/`@b` + digits render literally, so count them as visible.
                let vis = if hex == 3 { 0 } else { 2 + hex };
                Some((src, vis))
            }
            Some(c) => Some((format!("@{c}"), 2)), // unknown → literal '@' + c
        },
        c => Some((c.to_string(), 1)),
    }
}

/// The number of visible columns `s` occupies once its colour markup is rendered.
pub fn visible_width(s: &str) -> usize {
    let mut cs = s.chars().peekable();
    let mut w = 0;
    while let Some((_, vis)) = next_unit(&mut cs) {
        w += vis;
    }
    w
}

/// Truncate `s` to at most `max` *visible* columns without splitting a colour
/// token. Colour/reset codes (zero-width) are always kept as they are reached,
/// so leading/inline colour survives; the first visible unit that would exceed
/// `max` stops the walk (a 2-wide unknown code at the boundary is dropped whole,
/// never halved).
pub fn truncate_visible(s: &str, max: usize) -> String {
    let mut cs = s.chars().peekable();
    let mut out = String::with_capacity(s.len());
    let mut used = 0;
    while let Some((src, vis)) = next_unit(&mut cs) {
        if vis == 0 {
            out.push_str(&src);
            continue;
        }
        if used + vis > max {
            break;
        }
        out.push_str(&src);
        used += vis;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_width_is_char_count() {
        assert_eq!(visible_width("Hello"), 5);
        assert_eq!(visible_width(""), 0);
    }

    #[test]
    fn colour_codes_are_zero_width() {
        assert_eq!(visible_width("{RGod{x"), 3);
        assert_eq!(visible_width("@xf00God@r"), 3);
        assert_eq!(visible_width("{R{G{x"), 0);
    }

    #[test]
    fn escapes_count_as_one() {
        assert_eq!(visible_width("{{"), 1);
        assert_eq!(visible_width("@@"), 1);
        assert_eq!(visible_width("a{{b"), 3);
    }

    #[test]
    fn unknown_and_incomplete_codes_count_literally() {
        assert_eq!(visible_width("{z"), 2); // unknown brace code, literal
        assert_eq!(visible_width("@q"), 2); // unknown at-code, literal
        assert_eq!(visible_width("@xff"), 4); // incomplete hex (2 digits) → literal @x ff
    }

    #[test]
    fn truncate_keeps_colour_and_never_splits_a_token() {
        // "God" is 3 visible; the {R prefix and {x suffix ride along.
        assert_eq!(truncate_visible("{RGod{x", 3), "{RGod{x");
        // Cap below the visible length trims text but keeps the leading colour.
        assert_eq!(truncate_visible("{RGodzilla", 3), "{RGod");
        // A colour token exactly at the boundary is retained (zero-width).
        assert_eq!(truncate_visible("ab{xcd", 2), "ab{x");
    }

    #[test]
    fn truncate_plain_text() {
        assert_eq!(truncate_visible("VeryLongRace", 5), "VeryL");
        assert_eq!(truncate_visible("Hi", 5), "Hi");
    }

    #[test]
    fn truncate_drops_a_boundary_unknown_code_whole() {
        // "{z" is a 2-wide literal; with one column left it is dropped, not split.
        assert_eq!(truncate_visible("a{z", 2), "a");
    }
}
