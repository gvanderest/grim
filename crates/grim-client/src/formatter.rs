/// Format a room's full description.
pub fn format_room(
    name: &str,
    desc: &str,
    exits: &[String],
    occupants: &[String],
) -> String {
    let mut out = format!("{}\r\n{}", name, desc);
    if !exits.is_empty() {
        out.push_str(&format!("\r\nExits: {}", exits.join(", ")));
    }
    if !occupants.is_empty() {
        out.push_str(&format!(
            "\r\nAlso here: {}",
            occupants.join(", ")
        ));
    }
    out.push_str("\r\n> ");
    out
}

/// Format a look at a specific entity.
pub fn format_entity(name: &str, desc: &str) -> String {
    format!("{}\r\n{}\r\n", name, desc)
}

/// Format a say message broadcast to a room.
pub fn format_say(speaker: &str, text: &str) -> String {
    format!("{} says, '{}'\r\n", speaker, text)
}

/// Format a yell message broadcast to an area.
pub fn format_yell(speaker: &str, text: &str) -> String {
    format!("{} yells, '{}'\r\n", speaker, text)
}

/// Format an OOC message broadcast globally.
pub fn format_ooc(speaker: &str, text: &str) -> String {
    format!("[OOC] {}: {}\r\n", speaker, text)
}

/// Format a movement broadcast.
pub fn format_move(actor: &str, direction: &str, leaving: bool) -> String {
    if leaving {
        format!("{} leaves {}.\r\n", actor, direction)
    } else {
        format!("{} arrives.\r\n", actor)
    }
}

/// Format the who list.
pub fn format_who_list(players: &[String]) -> String {
    if players.is_empty() {
        "No other players online.\r\n".into()
    } else {
        let mut out = format!("Players online ({}):\r\n", players.len());
        for name in players {
            out.push_str(&format!("  {}\r\n", name));
        }
        out
    }
}

/// Format the where list (same-area players with room names).
pub fn format_where_list(entries: &[(String, String)]) -> String {
    if entries.is_empty() {
        "No other players in this area.\r\n".into()
    } else {
        let mut out = "Players in your area:\r\n".to_string();
        for (name, room) in entries {
            out.push_str(&format!("  {} in [{}]\r\n", name, room));
        }
        out
    }
}

/// Format the command list.
pub fn format_commands() -> String {
    let cmds = [
        "look [target]       — Look at the room or a specific target",
        "l [target]          — Shortcut for look",
        "say <text>          — Speak to everyone in the room",
        "'<text>             — Shortcut for say",
        "yell <text>         — Shout to everyone in the area",
        "ooc <text>          — Out-of-character global chat",
        "north / n           — Move north",
        "east / e            — Move east",
        "south / s           — Move south",
        "west / w            — Move west",
        "up / u              — Move up",
        "down / d            — Move down",
        "who                 — List players online",
        "where               — Show who's in your area",
        "commands / help     — Show this list",
        "quit / exit         — Disconnect from the game",
    ];
    let mut out = "Available commands:\r\n".to_string();
    for cmd in &cmds {
        out.push_str(&format!("  {}\r\n", cmd));
    }
    out
}

/// Format the MOTD.
pub fn format_motd() -> String {
    "Welcome to GRIMTIDE!\r\n\r\nAn evolving world of adventure awaits.\r\n\r\nPress ENTER to continue.\r\n"
        .into()
}

/// Format a linkdead announce.
pub fn format_linkdead(name: &str, reconnecting: bool) -> String {
    if reconnecting {
        format!("{} has reconnected.\r\n", name)
    } else {
        format!("{} has gone linkdead.\r\n", name)
    }
}

/// Format a prompt.
#[allow(dead_code)]
pub fn format_prompt() -> String {
    "> ".into()
}