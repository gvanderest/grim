//! Colour re-exports plus the locale-backed `tr` helper.
//!
//! The colour markup itself now lives in the `grim-color` crate, which has no
//! Bevy or serde dependency. This module re-exports it so existing call sites
//! (`grim::color::ansi`, `grim::color::convert_16color`, `grim::color::escape_codes`)
//! keep resolving, and additionally hosts `tr` / `tr!` — the locale-string
//! lookup — until the text catalog (`grim-text`) subsumes it.

pub use grim_color::{ansi, convert_16color, escape_codes, palette};

use serde_json::Value;
use std::sync::LazyLock;

/// Translate a locale key: convert `{X` 16-color codes to `@xRGB` format,
/// then substitute `%{var}` placeholders with the provided arguments.
///
/// Each argument value is escaped via [`escape_codes`] so a value can never be
/// interpreted as colour markup; templates keep their own colouring.
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
        out = out.replace(&pattern, &escape_codes(v));
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

#[cfg(test)]
mod tr_tests {
    use super::*;

    #[test]
    fn tr_unknown_key_returns_key() {
        assert_eq!(tr("no.such.key", &[]), "no.such.key");
    }

    #[test]
    fn tr_say_third_party_escapes_speech() {
        let out = tr(
            "social.say.third_party",
            &[("speaker", "Alice"), ("text", "{RHELLO")],
        );
        let rendered = ansi(&convert_16color(&out));
        assert!(
            rendered.contains("{RHELLO"),
            "spoken markup must stay literal: {rendered:?}"
        );
    }
}
