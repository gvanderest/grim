//! Rewriting the `{`-family 16-colour markup into the `@x`-family.
//!
//! Applied to all output before [`crate::ansi`], so colour reaches the renderer
//! in a single form drawn from GRIM's palette rather than the terminal theme.

use crate::palette::*;

/// Convert `{X` 16-color markup codes to `@xRGB` equivalents, using
/// our 75%-of-bright palette for dark colors and full RGB for bright.
/// `{x` (reset) maps to `@r`. Unknown `{X` patterns pass through literally.
/// This is applied to ALL output before `ansi()` to ensure color codes
/// use our palette rather than the terminal theme.
///
/// The `{{` escape passes through **unchanged**, because [`crate::ansi`] is the
/// single consumer of escapes. This function must be idempotent with respect to
/// them: it runs more than once over the same string (once during `tr`, again at
/// the protocol boundary), and if it collapsed `{{` to `{` then a second pass
/// would re-interpret the result as markup — which is exactly how an escaped
/// value could smuggle colour back in.
pub fn convert_16color(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut cs = s.chars().peekable();
    while let Some(ch) = cs.next() {
        if ch == '{' {
            match cs.next() {
                Some('{') => out.push_str("{{"),
                Some('k') => out.push_str(BLACK_DARK),
                Some('r') | Some('1') => out.push_str(RED_DARK),
                Some('g') | Some('2') => out.push_str(GREEN_DARK),
                Some('y') | Some('3') => out.push_str(YELLOW_DARK),
                Some('b') | Some('4') => out.push_str(BLUE_DARK),
                Some('m') | Some('5') => out.push_str(MAGENTA_DARK),
                Some('c') | Some('6') => out.push_str(CYAN_DARK),
                Some('w') | Some('7') => out.push_str(WHITE_DARK),
                Some('K') | Some('8') | Some('*') => out.push_str(BLACK_BRIGHT),
                Some('R') | Some('!') => out.push_str(RED_BRIGHT),
                Some('G') | Some('@') => out.push_str(GREEN_BRIGHT),
                Some('Y') | Some('#') => out.push_str(YELLOW_BRIGHT),
                Some('B') => out.push_str(BLUE_BRIGHT),
                Some('M') | Some('%') => out.push_str(MAGENTA_BRIGHT),
                Some('C') | Some('^') => out.push_str(CYAN_BRIGHT),
                Some('W') | Some('&') => out.push_str(WHITE_BRIGHT),
                Some('x' | 'X' | '9') => out.push_str(RESET),
                Some(other) => {
                    out.push('{');
                    out.push(other);
                }
                None => out.push('{'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod convert_tests {
    use crate::{ansi, convert_16color};

    #[test]
    fn convert_dark_red() {
        assert_eq!(convert_16color("{r"), "@x900");
    }

    #[test]
    fn convert_bright_red() {
        assert_eq!(convert_16color("{R"), "@xf00");
    }

    #[test]
    fn convert_reset() {
        assert_eq!(convert_16color("{x"), "@r");
        assert_eq!(convert_16color("{9"), "@r");
    }

    /// `convert_16color` forwards the escape rather than resolving it; `ansi` is
    /// the sole consumer. This makes it idempotent, which matters because it runs
    /// twice over the same string.
    #[test]
    fn convert_brace_escape_passes_through() {
        assert_eq!(convert_16color("{{"), "{{");
        assert_eq!(convert_16color(&convert_16color("{{")), "{{");
        assert_eq!(ansi("{{"), "{");
    }

    #[test]
    fn convert_unknown_passthrough() {
        assert_eq!(convert_16color("{z"), "{z");
    }

    #[test]
    fn convert_mixed_text() {
        let got = convert_16color("{RHello {rworld{x");
        assert_eq!(got, "@xf00Hello @x900world@r");
    }

    #[test]
    fn no_color_passthrough() {
        assert_eq!(convert_16color("hello world"), "hello world");
    }

    #[test]
    fn convert_leaves_placeholders_untouched() {
        let converted = convert_16color("{M%{speaker} says @r'@x909%{text}{x'");
        assert_eq!(converted, "@xf0f%{speaker} says @r'@x909%{text}@r'");
    }
}
