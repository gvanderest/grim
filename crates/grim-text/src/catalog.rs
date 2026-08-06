//! The catalog lookup: resolve a key to its (colour-converted, interpolated)
//! text. Kept in a sibling module so `lib.rs` stays declarations + re-exports.

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
        "error.unknown_command" => "Unknown command. Type 'commands' for a list.\n",
        "character.takeover" => "Someone else has logged into this character.\n",
        "character.default_description" => "A new adventurer.",
        other => return other.to_string(),
    }
    .to_string()
}
