//! The text catalog: every piece of author-facing text, addressed by a dotted
//! **key** (`social.say.third_party`). One namespace.
//!
//! # Why this is not `rust-i18n`
//!
//! GRIM's colour markup uses `{` (`{R`, `{x`), which collides head-on with the
//! `{var}` interpolation `rust-i18n`'s `t!` expects — a coloured string makes it
//! choke or silently mangle. So strings are addressed here with `%{var}`
//! placeholders, colour is converted separately by `grim-color`, and there is no
//! i18n runtime at all. This crate replaced two parallel systems (`rust-i18n` for
//! plain keys, a hand-rolled `tr` for coloured ones) that both read the same file.
//!
//! # Defaults, for now
//!
//! Defaults are inlined in [`default_string`] rather than loaded from disk. The
//! target design layers author overrides on top and merges `strings/<locale>/*.json`
//! with `templates/<locale>/<key>`; that arrives with the plugin-composition work
//! (a `Catalog` resource). Until then the lookup is a static function, which keeps
//! this crate Bevy-free and preserves current behaviour exactly. The `include_str!`
//! that used to reach out of the crate into the workspace-root `locales/` is gone.

use grim_color::{convert_16color, escape_codes};

/// Look up a catalog entry by `key`, convert its colour markup, and substitute
/// `%{name}` placeholders with the matching argument values.
///
/// Argument values are escaped via [`escape_codes`]: a value is data and must
/// never be read as markup. An unknown key resolves to the key itself, which
/// surfaces the missing entry rather than hiding it.
pub fn tr(key: &str, args: &[(&str, &str)]) -> String {
    let converted = convert_16color(&default_string(key));
    let mut out = converted;
    for (k, v) in args {
        let pattern = format!("%{{{}}}", k);
        out = out.replace(&pattern, &escape_codes(v));
    }
    out
}

/// The built-in default text for a key, or the key itself when unknown.
fn default_string(key: &str) -> String {
    match key {
        "login.prompt" => "Enter your character name or email address: ",
        "login.wrong_password" => "Invalid password.\nEnter your character name or email address: ",
        "social.say.first_party" => "{MYou say {x'{m%{text}{x'\n",
        "social.say.third_party" => "{M%{speaker} says {x'{m%{text}{x'\n",
        other => return other.to_string(),
    }
    .to_string()
}

/// Look up and interpolate a catalog key with `t!()`-style syntax.
///
/// ```ignore
/// tr!("login.prompt");
/// tr!("social.say.third_party", speaker = speaker, text = text);
/// ```
#[macro_export]
macro_rules! tr {
    ($key:expr $(, $arg:ident = $val:expr)* $(,)?) => {{
        let args: &[(&str, &str)] = &[$((stringify!($arg), $val.as_ref())),*];
        $crate::tr($key, args)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use grim_color::{ansi, convert_16color};

    #[test]
    fn unknown_key_returns_key() {
        assert_eq!(tr("no.such.key", &[]), "no.such.key");
    }

    #[test]
    fn plain_key_has_no_markup() {
        assert_eq!(
            tr("login.prompt", &[]),
            "Enter your character name or email address: "
        );
    }

    #[test]
    fn wrong_password_keeps_newline() {
        assert_eq!(
            tr("login.wrong_password", &[]),
            "Invalid password.\nEnter your character name or email address: "
        );
    }

    #[test]
    fn say_first_party_interpolates() {
        let out = tr("social.say.first_party", &[("text", "hello")]);
        assert_eq!(out, "@xf0fYou say @r'@x909hello@r'\n");
    }

    #[test]
    fn say_third_party_interpolates() {
        let out = tr(
            "social.say.third_party",
            &[("speaker", "Alice"), ("text", "hi")],
        );
        assert_eq!(out, "@xf0fAlice says @r'@x909hi@r'\n");
    }

    #[test]
    fn interpolated_value_cannot_inject_colour() {
        let out = tr(
            "social.say.third_party",
            &[("speaker", "Alice"), ("text", "{RHELLO")],
        );
        let rendered = ansi(&convert_16color(&out));
        assert!(
            rendered.contains("{RHELLO"),
            "spoken markup must render literally: {rendered:?}"
        );
    }

    #[test]
    fn macro_matches_function() {
        let text = "hi";
        assert_eq!(
            tr!("social.say.first_party", text = text),
            tr("social.say.first_party", &[("text", "hi")])
        );
    }

    #[test]
    fn macro_with_no_args() {
        assert_eq!(tr!("login.prompt"), tr("login.prompt", &[]));
    }
}
