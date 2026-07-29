use serde_json::Value;
use std::fmt::Write;
use std::sync::LazyLock;

/// Convert 16-color terminal markup (`{code`) and 24-bit hex color markup
/// (`@xRGB` / `@bRGB`) to ANSI escape sequences.
///
/// # 16-color codes: `{` + one char
///
/// | Input       | Color                    | ANSI  |
/// |-------------|--------------------------|-------|
/// | `{x` / `{9` | Reset                    | 0     |
/// | `{k` / `{1` | Black                    | 30    |
/// | `{r` / `{1` | Red                      | 31    |
/// | `{g` / `{2` | Green                    | 32    |
/// | `{y` / `{3` | Yellow                   | 33    |
/// | `{b` / `{4` | Blue                     | 34    |
/// | `{m` / `{5` | Magenta                  | 35    |
/// | `{c` / `{6` | Cyan                     | 36    |
/// | `{w` / `{7` | White                    | 37    |
/// | `{K` / `{8` / `{*` | Bright Black (Grey) | 90    |
/// | `{R` / `{!` | Bright Red               | 91    |
/// | `{G` / `{@` | Bright Green             | 92    |
/// | `{Y` / `{#` | Bright Yellow            | 93    |
/// | `{B`        | Bright Blue              | 94    |
/// | `{M` / `{%` | Bright Magenta           | 95    |
/// | `{C` / `{^` | Bright Cyan             | 96    |
/// | `{W` / `{&` | Bright White             | 97    |
///
/// # 24-bit hex codes: `@` + prefix + 3 hex digits
///
/// | Code     | Effect                         |
/// |----------|--------------------------------|
/// | `@r`     | Reset                          |
/// | `@xRGB`  | Foreground (3 hex digits, 12→24 bit via nibble×17) |
/// | `@bRGB`  | Background (same scaling)       |
///
/// # Escaping
///
/// | Input | Output  |
/// |-------|---------|
/// | `{{`  | `{`     |
/// | `@@`  | `@`     |
///
/// Unknown codes (e.g. `{z`, `{-`, `@q`) pass through as literal text.
pub fn ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 64);
    let mut cs = input.chars().peekable();

    while let Some(ch) = cs.next() {
        match ch {
            '{' => match cs.next() {
                Some('{') => out.push('{'),
                Some('x' | 'X' | '9') => push_reset(&mut out),
                Some(c @ '1'..='8') => push_ansi(&mut out, ansi_16_num(c)),
                Some(
                    c @ 'k' | c @ 'r' | c @ 'g' | c @ 'y' | c @ 'b' | c @ 'm' | c @ 'c' | c @ 'w',
                ) => push_ansi(&mut out, ansi_16_dark(c)),
                Some(
                    c @ 'K' | c @ 'R' | c @ 'G' | c @ 'Y' | c @ 'B' | c @ 'M' | c @ 'C' | c @ 'W',
                ) => push_ansi(&mut out, ansi_16_bright(c)),
                Some(c @ '!' | c @ '@' | c @ '#' | c @ '%' | c @ '^' | c @ '&' | c @ '*') => {
                    push_ansi(&mut out, ansi_16_symbol(c))
                }
                Some(other) => {
                    out.push('{');
                    out.push(other);
                }
                None => out.push('{'),
            },
            '@' => match cs.next() {
                Some('@') => out.push('@'),
                Some('r' | 'R') => push_reset(&mut out),
                Some('x' | 'X') => {
                    let mut buf = ['\0'; 3];
                    let mut n = 0;
                    for slot in &mut buf {
                        match cs.peek() {
                            Some(c) if c.is_ascii_hexdigit() => {
                                *slot = *c;
                                cs.next();
                                n += 1;
                            }
                            _ => break,
                        }
                    }
                    if n == 3 {
                        push_24bit_fg(
                            &mut out,
                            buf[0].to_digit(16).unwrap() as u8 * 17,
                            buf[1].to_digit(16).unwrap() as u8 * 17,
                            buf[2].to_digit(16).unwrap() as u8 * 17,
                        );
                    } else {
                        out.push('@');
                        out.push('x');
                        for &d in buf.iter().take(n) {
                            out.push(d);
                        }
                    }
                }
                Some('b' | 'B') => {
                    let mut buf = ['\0'; 3];
                    let mut n = 0;
                    for slot in &mut buf {
                        match cs.peek() {
                            Some(c) if c.is_ascii_hexdigit() => {
                                *slot = *c;
                                cs.next();
                                n += 1;
                            }
                            _ => break,
                        }
                    }
                    if n == 3 {
                        push_24bit_bg(
                            &mut out,
                            buf[0].to_digit(16).unwrap() as u8 * 17,
                            buf[1].to_digit(16).unwrap() as u8 * 17,
                            buf[2].to_digit(16).unwrap() as u8 * 17,
                        );
                    } else {
                        out.push('@');
                        out.push('b');
                        for &d in buf.iter().take(n) {
                            out.push(d);
                        }
                    }
                }
                Some(other) => {
                    out.push('@');
                    out.push(other);
                }
                None => out.push('@'),
            },
            _ => out.push(ch),
        }
    }

    out
}

fn ansi_16_dark(c: char) -> u8 {
    match c {
        'k' => 30,
        'r' => 31,
        'g' => 32,
        'y' => 33,
        'b' => 34,
        'm' => 35,
        'c' => 36,
        'w' => 37,
        _ => unreachable!(),
    }
}

fn ansi_16_bright(c: char) -> u8 {
    ansi_16_dark(c.to_ascii_lowercase()) + 60
}

fn ansi_16_num(c: char) -> u8 {
    match c {
        '1' => 31,
        '2' => 32,
        '3' => 33,
        '4' => 34,
        '5' => 35,
        '6' => 36,
        '7' => 37,
        '8' => 90,
        _ => unreachable!(),
    }
}

fn ansi_16_symbol(c: char) -> u8 {
    match c {
        '!' => 91,
        '@' => 92,
        '#' => 93,
        '%' => 95,
        '^' => 96,
        '&' => 97,
        '*' => 90,
        _ => unreachable!(),
    }
}

fn push_reset(out: &mut String) {
    out.push_str("\x1b[0m");
}

fn push_ansi(out: &mut String, code: u8) {
    let _ = write!(out, "\x1b[{code}m");
}

fn push_24bit_fg(out: &mut String, r: u8, g: u8, b: u8) {
    let _ = write!(out, "\x1b[38;2;{r};{g};{b}m");
}

fn push_24bit_bg(out: &mut String, r: u8, g: u8, b: u8) {
    let _ = write!(out, "\x1b[48;2;{r};{g};{b}m");
}

// ─── Runtime translation (tr!) ────────────────────────────────────────

/// Translate a locale key: convert `{X` 16-color codes to `@xRGB` format,
/// then substitute `%{var}` placeholders with the provided arguments.
///
/// This is a runtime replacement for `t!()` when the string contains color
/// markup (`{r`, `{R`, etc.) that would conflict with i18n's `{name}`
/// interpolation syntax. By using `@xRGB` instead, there's no brace conflict.
///
/// # Example
///
/// Locale: `"{RHello there %{name}"`
/// Call: `tr("some.key", &[("name", "Alice")])`
/// Result: `"@xf00Hello there Alice"`
pub fn tr(key: &str, args: &[(&str, &str)]) -> String {
    let raw = locale_string(key);
    let converted = convert_16color(&raw);
    let mut out = converted;
    for (k, v) in args {
        let pattern = format!("%{{{}}}", k);
        out = out.replace(&pattern, v);
    }
    out
}

/// Translate with color-code conversion. Wraps `tr()` with `t!()`-style syntax.
///
/// # Examples
///
/// ```ignore
/// tr!("social.say.first_party", text = text);
/// tr!("social.say.third_party", speaker = speaker, text = text);
/// ```
#[macro_export]
macro_rules! tr {
    ($key:expr $(, $arg:ident = $val:expr)* $(,)?) => {{
        let args: &[(&str, &str)] = &[$((stringify!($arg), $val.as_ref())),*];
        $crate::color::tr($key, args)
    }};
}

fn locale_string(key: &str) -> String {
    let data = locale_data();
    data.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| key.to_string())
}

fn locale_data() -> &'static Value {
    static LOCALE: LazyLock<Value> = LazyLock::new(|| {
        let content = include_str!("../../../locales/en.json");
        serde_json::from_str(content)
            .unwrap_or_else(|e| panic!("failed to parse locale file: {}", e))
    });
    &LOCALE
}

/// Convert `{X` 16-color markup codes to `@xRGB` equivalents, using
/// our 75%-of-bright palette for dark colors and full RGB for bright.
/// `{x` (reset) maps to `@r`. Unknown `{X` patterns pass through literally.
/// This is applied to ALL output before `ansi()` to ensure color codes
/// use our palette rather than the terminal theme.
pub fn convert_16color(s: &str) -> String {
    use crate::palette::*;
    let mut out = String::with_capacity(s.len());
    let mut cs = s.chars().peekable();
    while let Some(ch) = cs.next() {
        if ch == '{' {
            match cs.next() {
                Some('{') => out.push('{'),
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
mod tr_tests {
    use super::*;

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

    #[test]
    fn convert_brace_escape() {
        assert_eq!(convert_16color("{{"), "{");
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
    fn tr_resolves_vars() {
        let converted = convert_16color("{M%{speaker} says @r'@x909%{text}{x'");
        assert_eq!(converted, "@xf0f%{speaker} says @r'@x909%{text}@r'");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Passthrough ──

    #[test]
    fn plain_text() {
        assert_eq!(ansi("hello world"), "hello world");
    }
    #[test]
    fn empty() {
        assert_eq!(ansi(""), "");
    }
    #[test]
    fn no_codes() {
        assert_eq!(ansi("just text\n"), "just text\n");
    }

    // ── 16-color terminal ──

    #[test]
    fn reset_codes() {
        assert_eq!(ansi("{x"), "\x1b[0m");
        assert_eq!(ansi("{X"), "\x1b[0m");
        assert_eq!(ansi("{9"), "\x1b[0m");
    }

    #[test]
    fn dark_colors() {
        assert_eq!(ansi("{k"), "\x1b[30m");
        assert_eq!(ansi("{r"), "\x1b[31m");
        assert_eq!(ansi("{g"), "\x1b[32m");
        assert_eq!(ansi("{y"), "\x1b[33m");
        assert_eq!(ansi("{b"), "\x1b[34m");
        assert_eq!(ansi("{m"), "\x1b[35m");
        assert_eq!(ansi("{c"), "\x1b[36m");
        assert_eq!(ansi("{w"), "\x1b[37m");
    }

    #[test]
    fn bright_colors() {
        assert_eq!(ansi("{K"), "\x1b[90m");
        assert_eq!(ansi("{R"), "\x1b[91m");
        assert_eq!(ansi("{G"), "\x1b[92m");
        assert_eq!(ansi("{Y"), "\x1b[93m");
        assert_eq!(ansi("{B"), "\x1b[94m");
        assert_eq!(ansi("{M"), "\x1b[95m");
        assert_eq!(ansi("{C"), "\x1b[96m");
        assert_eq!(ansi("{W"), "\x1b[97m");
    }

    #[test]
    fn numeric_aliases() {
        assert_eq!(ansi("{1"), "\x1b[31m");
        assert_eq!(ansi("{2"), "\x1b[32m");
        assert_eq!(ansi("{3"), "\x1b[33m");
        assert_eq!(ansi("{4"), "\x1b[34m");
        assert_eq!(ansi("{5"), "\x1b[35m");
        assert_eq!(ansi("{6"), "\x1b[36m");
        assert_eq!(ansi("{7"), "\x1b[37m");
        assert_eq!(ansi("{8"), "\x1b[90m");
    }

    #[test]
    fn symbol_aliases() {
        assert_eq!(ansi("{!"), "\x1b[91m");
        assert_eq!(ansi("{@"), "\x1b[92m");
        assert_eq!(ansi("{#"), "\x1b[93m");
        assert_eq!(ansi("{%"), "\x1b[95m");
        assert_eq!(ansi("{^"), "\x1b[96m");
        assert_eq!(ansi("{&"), "\x1b[97m");
        assert_eq!(ansi("{*"), "\x1b[90m");
    }

    #[test]
    fn mixed_16color_text() {
        let got = ansi("Hello {RAlice{r, welcome to {cThe Tavern{x!");
        assert_eq!(
            got,
            "Hello \x1b[91mAlice\x1b[31m, welcome to \x1b[36mThe Tavern\x1b[0m!"
        );
    }

    #[test]
    fn unknown_passthrough() {
        assert_eq!(ansi("{z"), "{z");
        assert_eq!(ansi("{-"), "{-");
        assert_eq!(ansi("{"), "{");
    }

    // ── 24-bit hex color ──

    #[test]
    fn hex_reset() {
        assert_eq!(ansi("@r"), "\x1b[0m");
    }

    #[test]
    fn hex_foreground() {
        assert_eq!(ansi("@x000"), "\x1b[38;2;0;0;0m");
        assert_eq!(ansi("@xfff"), "\x1b[38;2;255;255;255m");
        assert_eq!(ansi("@xf00"), "\x1b[38;2;255;0;0m");
        assert_eq!(ansi("@x0f0"), "\x1b[38;2;0;255;0m");
        assert_eq!(ansi("@x00f"), "\x1b[38;2;0;0;255m");
        assert_eq!(ansi("@x888"), "\x1b[38;2;136;136;136m");
    }

    #[test]
    fn hex_background() {
        assert_eq!(ansi("@b000"), "\x1b[48;2;0;0;0m");
        assert_eq!(ansi("@bfff"), "\x1b[48;2;255;255;255m");
        assert_eq!(ansi("@b800"), "\x1b[48;2;136;0;0m");
    }

    #[test]
    fn hex_uppercase() {
        assert_eq!(ansi("@xFFF"), "\x1b[38;2;255;255;255m");
    }

    #[test]
    fn hex_mixed_text() {
        let got = ansi("This is @xf00red@x000 text.");
        assert_eq!(got, "This is \x1b[38;2;255;0;0mred\x1b[38;2;0;0;0m text.");
    }

    #[test]
    fn hex_not_enough_digits() {
        assert_eq!(ansi("@xab"), "@xab");
        assert_eq!(ansi("@x1"), "@x1");
        assert_eq!(ansi("@x"), "@x");
    }

    #[test]
    fn hex_unknown_code() {
        assert_eq!(ansi("@q"), "@q");
    }
    #[test]
    fn hex_trailing_at() {
        assert_eq!(ansi("test@"), "test@");
    }

    // ── Escaping ──

    #[test]
    fn escape_brace() {
        assert_eq!(ansi("{{"), "{");
    }
    #[test]
    fn escape_at() {
        assert_eq!(ansi("@@"), "@");
    }
    #[test]
    fn escaped_brace_no_color() {
        assert_eq!(ansi("{{c"), "{c");
        assert_eq!(ansi("{{C"), "{C");
    }
    #[test]
    fn escaped_at_no_color() {
        assert_eq!(ansi("@@xf00"), "@xf00");
    }
    #[test]
    fn mixed_escape_and_color() {
        let got = ansi("{{c{R real text{x");
        assert_eq!(got, "{c\x1b[91m real text\x1b[0m");
    }

    // ── Integration ──

    #[test]
    fn mixed_formats() {
        let got = ansi("{R@xfffMIXED{x");
        assert_eq!(got, "\x1b[91m\x1b[38;2;255;255;255mMIXED\x1b[0m");
    }

    #[test]
    fn newlines_preserved() {
        assert_eq!(ansi("line1\nline2"), "line1\nline2");
        assert_eq!(
            ansi("line1\n{Rred line{x\nline2"),
            "line1\n\x1b[91mred line\x1b[0m\nline2"
        );
    }
}
