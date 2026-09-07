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
        "inventory.empty" => "You are carrying nothing.\n",
        "equipment.empty" => "You are wearing nothing.\n",
        "item.pickup.first" => "You pick up %{short}\n",
        "item.pickup.third" => "%{name} picks up %{short}\n",
        "item.drop.first" => "You drop %{short}\n",
        "item.drop.third" => "%{name} drops %{short}\n",
        "item.get.not_found" => "You don't see that here.\n",
        "item.drop.not_carried" => "You aren't carrying that.\n",
        "inventory.list.header" => "You are carrying:\n",
        "inventory.list.row" => "  %{short}\n",
        "item.give.first" => "You give %{short} to %{other}\n",
        "item.give.target" => "%{name} gives you %{short}\n",
        "item.give.third" => "%{name} gives %{short} to %{other}\n",
        "item.give.not_carried" => "You aren't carrying that.\n",
        "item.give.not_here" => "They aren't here.\n",
        "item.give.refused" => "They don't want that item.\n",
        "item.give.self" => "You can't give something to yourself.\n",
        "item.steal.first" => "You steal %{short} from %{other}\n",
        "item.steal.target" => "%{name} steals your %{short}\n",
        "item.steal.third" => "%{name} steals %{short} from %{other}\n",
        "item.steal.not_found" => "They aren't carrying that.\n",
        "item.steal.not_here" => "They aren't here.\n",
        "item.steal.self" => "You can't steal from yourself.\n",
        "look.pack.header" => "%{name} is carrying:\n",
        "look.pack.empty" => "%{name} is carrying nothing.\n",
        "editor.enter.header" => {
            "Editing description. Type lines to add them; commands start with @.\n"
        }
        "editor.enter.row" => "  %{num}. %{line}\n",
        "editor.enter.empty" => "  (empty)\n",
        "editor.enter.help" => {
            "  @save — save and exit | @exit — discard and exit | @clear — empty the buffer\n"
        }
        "editor.cleared" => "Buffer cleared.\n",
        "editor.unknown" => "Unknown editor command. Use @save, @exit, or @clear.\n",
        "desc.saved" => "Your description has been saved.\n",
        "desc.edit_cancelled" => "Edit cancelled, description unchanged.\n",
        "commands.desc" => "desc [clear|+/-|edit] — View or edit your description paragraphs",
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
        "ban.banned.ip" => "Your IP address has been banned from connecting to the server.\n",
        "ban.banned.account" => "Your account has been banned from connecting to the server.\n",
        "ban.banned.character" => "Your character has been banned from connecting to the server.\n",
        "ban.list.empty" => "No bans.\n",
        "ban.list.header" => "Bans (%{total}):\n",
        "ban.list.row" => "  %{scope} %{pattern} (banned %{date} by %{author})\n",
        "ban.added" => "Ban added: %{scope} %{pattern}\n",
        "ban.exists" => "That ban already exists.\n",
        "ban.removed" => "Ban removed: %{scope} %{pattern}\n",
        "ban.not_found" => "No such ban.\n",
        "ban.save_failed" => "Ban could not be saved; nothing changed.\n",
        "ban.invalid_pattern" => "Invalid pattern for %{scope} ban.\n",
        other => return other.to_string(),
    }
    .to_string()
}
