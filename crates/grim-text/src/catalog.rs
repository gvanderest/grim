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
///
/// Placeholder names must NOT begin with a markup-code character (`k r g y b
/// m c w` either case, `1`-`8`, `! @ # % ^ & *`, `x X 9`): `convert_16color`
/// runs before substitution, so `%{count}` would lose its `{c` to the cyan
/// code. Prefer neutral initials (`total`, `name`, `addr`, `text`).
fn default_string(key: &str) -> String {
    match key {
        "login.prompt" => "Enter your character name or email address: ",
        "login.wrong_password" => "Invalid password.\nEnter your character name or email address: ",
        "social.say.first_party" => "{MYou say {x'{m%{text}{x'\n",
        "social.say.third_party" => "{M%{speaker} says {x'{m%{text}{x'\n",
        "channel.say.first_party" => "{MYou say {x'{m%{text}{x'\n",
        "channel.say.third_party" => "{M%{speaker} says {x'{m%{text}{x'\n",
        "channel.yell.first_party" => "{MYou yell, '{m%{text}{x'\n",
        "channel.yell.third_party" => "{M%{speaker} yells, '{m%{text}{x'\n",
        "channel.ooc.first_party" => "[OOC] You: %{text}\n",
        "channel.ooc.third_party" => "[OOC] %{speaker}: %{text}\n",
        "error.unknown_command" => "Unknown command. Type 'commands' for a list.\n",
        "character.takeover" => "Someone else has logged into this character.\n",
        "character.default_description" => "A new adventurer.",
        "room.presence.standing" => "%{name} is standing here.",
        "room.presence.here" => "%{name} is here.",
        "finger.not_found" => "You don't know anyone by that name.\n",
        "desc.cleared" => "Your description has been cleared.\n",
        "desc.added" => "Line added to your description.\n",
        "desc.removed" => "Last line removed from your description.\n",
        "desc.empty" => "Your description is already empty.\n",
        "sockets.empty" => "No connections.\n",
        "sockets.header" => "Sockets connected (%{total}):\n",
        "sockets.row" => "  [%{id}] %{addr} %{state} %{name} (%{account})\n",
        "sockets.state.ingame" => "InGame",
        "sockets.state.login" => "Login",
        "sockets.state.password" => "Password",
        "sockets.state.confirm" => "Confirm",
        "sockets.state.select" => "Select",
        "sockets.state.newchar" => "NewChar",
        "sockets.state.gender" => "Gender",
        "sockets.state.race" => "Race",
        "sockets.state.class" => "Class",
        "sockets.state.motd" => "MOTD",
        other => return other.to_string(),
    }
    .to_string()
}
