use crate::color;

/// Format a room's full description.
pub fn format_room(name: &str, desc: &str, exits: &[String], occupants: &[String]) -> String {
    let mut out = format!("{}\r\n{}", name, desc);
    if !exits.is_empty() {
        out.push_str(&format!("\r\nExits: {}", exits.join(", ")));
    }
    if !occupants.is_empty() {
        out.push_str(&format!("\r\nAlso here: {}", occupants.join(", ")));
    }
    out.push_str("\r\n");
    color::ansi(&out)
}

/// Format a look at a specific entity.
pub fn format_entity(name: &str, desc: &str) -> String {
    color::ansi(&format!("{}\r\n{}\r\n", name, desc))
}

/// Format a say message broadcast to a room.
pub fn format_say(speaker: &str, text: &str) -> String {
    color::ansi(&format!("{} says, '{}'\r\n", speaker, text))
}

/// Format a yell message broadcast to an area.
pub fn format_yell(speaker: &str, text: &str) -> String {
    color::ansi(&format!("{} yells, '{}'\r\n", speaker, text))
}

/// Format an OOC message broadcast globally.
pub fn format_ooc(speaker: &str, text: &str) -> String {
    color::ansi(&format!("[OOC] {}: {}\r\n", speaker, text))
}

/// Format a movement broadcast.
pub fn format_move(actor: &str, direction: &str, leaving: bool) -> String {
    if leaving {
        color::ansi(&format!("{} leaves {}.\r\n", actor, direction))
    } else {
        color::ansi(&format!("{} arrives.\r\n", actor))
    }
}

/// Format the who list.
pub fn format_who_list(players: &[String]) -> String {
    if players.is_empty() {
        color::ansi("No other players online.\r\n")
    } else {
        let mut out = format!("Players online ({}):\r\n", players.len());
        for name in players {
            out.push_str(&format!("  {}\r\n", name));
        }
        color::ansi(&out)
    }
}

/// Format the where list (same-area players with room names).
pub fn format_where_list(entries: &[(String, String)]) -> String {
    if entries.is_empty() {
        color::ansi("No other players in this area.\r\n")
    } else {
        let mut out = "Players in your area:\r\n".to_string();
        for (name, room) in entries {
            out.push_str(&format!("  {} in [{}]\r\n", name, room));
        }
        color::ansi(&out)
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
    color::ansi(&out)
}

/// Format the MOTD.
pub fn format_motd() -> String {
    color::ansi(include_str!("../../../assets/motd.txt"))
}

/// Format a linkdead announce.
pub fn format_linkdead(name: &str, reconnecting: bool) -> String {
    if reconnecting {
        color::ansi(&format!("{} has reconnected.\r\n", name))
    } else {
        color::ansi(&format!("{} has gone linkdead.\r\n", name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_room ──────────────────────────────────────────────────────────────

    #[test]
    fn room_with_exits_and_occupants() {
        let exits = vec!["north".into(), "east".into()];
        let occs = vec!["Alice".into(), "Bob".into()];
        let got = format_room("The Tavern", "A warm room.", &exits, &occs);
        assert!(got.starts_with("The Tavern\r\nA warm room."));
        assert!(got.contains("Exits: north, east"));
        assert!(got.contains("Also here: Alice, Bob"));
        assert!(got.ends_with("\r\n"));
    }

    #[test]
    fn room_no_exits() {
        let got = format_room("Void", "Empty.", &[], &["Guard".into()]);
        assert!(got.starts_with("Void\r\nEmpty."));
        assert!(!got.contains("Exits:"));
        assert!(got.contains("Also here: Guard"));
    }

    #[test]
    fn room_no_occupants() {
        let exits = vec!["south".into()];
        let got = format_room("Cell", "Dark.", &exits, &[]);
        assert!(got.contains("Exits: south"));
        assert!(!got.contains("Also here:"));
    }

    #[test]
    fn room_empty_both() {
        let got = format_room("Empty", "Nothing.", &[], &[]);
        assert_eq!(got, "Empty\r\nNothing.\r\n");
    }

    // ── format_entity ────────────────────────────────────────────────────────────

    #[test]
    fn entity_basic() {
        assert_eq!(
            format_entity("Sword", "A rusty blade."),
            "Sword\r\nA rusty blade.\r\n"
        );
    }

    #[test]
    fn entity_empty_desc() {
        assert_eq!(format_entity("Nothing", ""), "Nothing\r\n\r\n");
    }

    // ── format_say ───────────────────────────────────────────────────────────────

    #[test]
    fn say_basic() {
        assert_eq!(
            format_say("Alice", "hello there"),
            "Alice says, 'hello there'\r\n"
        );
    }

    #[test]
    fn say_empty_text() {
        assert_eq!(format_say("Bob", ""), "Bob says, ''\r\n");
    }

    // ── format_yell ──────────────────────────────────────────────────────────────

    #[test]
    fn yell_basic() {
        assert_eq!(
            format_yell("Guard", "intruder"),
            "Guard yells, 'intruder'\r\n"
        );
    }

    #[test]
    fn yell_empty_text() {
        assert_eq!(format_yell("Echo", ""), "Echo yells, ''\r\n");
    }

    // ── format_ooc ───────────────────────────────────────────────────────────────

    #[test]
    fn ooc_basic() {
        assert_eq!(
            format_ooc("Alice", "anyone at the tavern?"),
            "[OOC] Alice: anyone at the tavern?\r\n"
        );
    }

    #[test]
    fn ooc_empty_text() {
        assert_eq!(format_ooc("Bob", ""), "[OOC] Bob: \r\n");
    }

    // ── format_move ──────────────────────────────────────────────────────────────

    #[test]
    fn move_leaving() {
        assert_eq!(
            format_move("Alice", "north", true),
            "Alice leaves north.\r\n"
        );
    }

    #[test]
    fn move_arriving() {
        assert_eq!(format_move("Bob", "east", false), "Bob arrives.\r\n");
    }

    // ── format_who_list ──────────────────────────────────────────────────────────

    #[test]
    fn who_empty() {
        assert_eq!(format_who_list(&[]), "No other players online.\r\n");
    }

    #[test]
    fn who_populated() {
        let players = vec!["Alice".into(), "Bob".into(), "Charlie".into()];
        let got = format_who_list(&players);
        assert!(got.starts_with("Players online (3):\r\n"));
        assert!(got.contains("  Alice\r\n"));
        assert!(got.contains("  Bob\r\n"));
        assert!(got.contains("  Charlie\r\n"));
    }

    // ── format_where_list ────────────────────────────────────────────────────────

    #[test]
    fn where_empty() {
        assert_eq!(format_where_list(&[]), "No other players in this area.\r\n");
    }

    #[test]
    fn where_populated() {
        let entries = vec![
            ("Alice".into(), "Tavern".into()),
            ("Bob".into(), "Garden".into()),
        ];
        let got = format_where_list(&entries);
        assert!(got.starts_with("Players in your area:\r\n"));
        assert!(got.contains("  Alice in [Tavern]\r\n"));
        assert!(got.contains("  Bob in [Garden]\r\n"));
    }

    // ── format_commands ──────────────────────────────────────────────────────────

    #[test]
    fn commands_contains_known_entries() {
        let got = format_commands();
        assert!(got.starts_with("Available commands:\r\n"));
        assert!(got.contains("say <text>"));
        assert!(got.contains("who"));
        assert!(got.contains("quit / exit"));
        assert!(got.ends_with("\r\n"));
    }

    // ── format_motd ──────────────────────────────────────────────────────────────

    #[test]
    fn motd_non_empty() {
        let got = format_motd();
        assert!(!got.is_empty());
    }

    // ── format_linkdead ──────────────────────────────────────────────────────────

    #[test]
    fn linkdead_gone() {
        assert_eq!(
            format_linkdead("Alice", false),
            "Alice has gone linkdead.\r\n"
        );
    }

    #[test]
    fn linkdead_reconnected() {
        assert_eq!(format_linkdead("Bob", true), "Bob has reconnected.\r\n");
    }
}
