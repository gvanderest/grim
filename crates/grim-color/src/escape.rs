//! Neutralising untrusted values so they render literally instead of as markup.

/// Escape colour markup in a value so it renders literally.
///
/// Catalog templates are authored, and may colour themselves freely. Argument
/// values are data — a character name, a line the player typed — and must never
/// be interpreted as markup, or `say {RHELLO` turns the room's text red.
///
/// Escaping is doubling: `{` → `{{` and `@` → `@@`, both of which [`crate::ansi`]
/// resolves back to a single literal character.
pub fn escape_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '{' => out.push_str("{{"),
            '@' => out.push_str("@@"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod escape_tests {
    use crate::escape_codes;

    #[test]
    fn escape_doubles_markup_introducers() {
        assert_eq!(escape_codes("{RHELLO"), "{{RHELLO");
        assert_eq!(escape_codes("@xf00HELLO"), "@@xf00HELLO");
        assert_eq!(escape_codes("plain text"), "plain text");
    }
}

/// Colour-code injection regression tests.
///
/// An interpolated value is data, never markup. These exercise the whole output
/// path, including the second `convert_16color` pass the protocol layer applies,
/// because an escape that only survives one pass is no escape at all.
#[cfg(test)]
mod injection_tests {
    use crate::{ansi, convert_16color, escape_codes};

    fn render(s: &str) -> String {
        ansi(&convert_16color(s))
    }

    #[test]
    fn escaped_16color_code_renders_literally() {
        let rendered = render(&escape_codes("{RHELLO"));
        assert_eq!(rendered, "{RHELLO");
        assert!(!rendered.contains('\x1b'), "no escape sequence emitted");
    }

    #[test]
    fn escaped_24bit_code_renders_literally() {
        let rendered = render(&escape_codes("@xf00HELLO"));
        assert_eq!(rendered, "@xf00HELLO");
        assert!(!rendered.contains('\x1b'), "no escape sequence emitted");
    }

    #[test]
    fn escaped_value_survives_a_second_conversion_pass() {
        // A catalog converts once, the protocol layer converts again.
        let escaped = escape_codes("{RHELLO");
        let twice = convert_16color(&convert_16color(&escaped));
        assert_eq!(ansi(&twice), "{RHELLO");
    }

    #[test]
    fn user_escape_attempt_stays_literal() {
        // A player typing the escape itself gets the escape, not a brace pair
        // that later renders as markup.
        assert_eq!(render(&escape_codes("{{R")), "{{R");
    }

    #[test]
    fn template_colour_still_applies_around_escaped_value() {
        // The regression must not disarm the catalog's own markup.
        let composed = format!("{}{}", "{R", escape_codes("{ghi"));
        let rendered = render(&composed);
        assert!(rendered.starts_with('\x1b'), "template colour survives");
        assert!(
            rendered.ends_with("{ghi"),
            "value stays literal: {rendered}"
        );
    }
}
