use grim_color::escape_codes;
use grim_text::tr;

/// Room identity ids appended to a room title for admins only.
pub struct RoomDebugIds<'a> {
    /// Boot-local entity id (`Entity::to_bits`).
    pub entity: u64,
    /// Stable grim id. (Today this is the room's `GrimId`; becomes the base62
    /// Grim ID once that lands — see `docs/adr/0001`.)
    pub grim: &'a str,
    /// Human-facing slug (`friendly_id`).
    pub slug: &'a str,
}

/// A room's title line. Plain for normal players; admins additionally see the
/// entity id, grim id, and slug for building/debugging:
/// `Town Square (entity:… grim:… slug:…)`.
pub fn room_title(name: &str, debug: Option<RoomDebugIds>) -> String {
    match debug {
        Some(d) => format!(
            "{name} (entity:{} grim:{} slug:{})",
            d.entity, d.grim, d.slug
        ),
        None => name.to_string(),
    }
}

/// Format a room's full description.
pub fn format_room(name: &str, desc: &str, exits: &[String], occupants: &[String]) -> String {
    let mut out = format!("{}\n{}", name, desc);
    if !exits.is_empty() {
        out.push_str(&format!("\nExits: {}", exits.join(", ")));
    }
    if !occupants.is_empty() {
        out.push_str(&format!("\nAlso here: {}", occupants.join(", ")));
    }
    out.push('\n');
    out
}

/// Format a look at a specific entity.
pub fn format_entity(name: &str, desc: &str) -> String {
    format!("{}\n{}\n", name, desc)
}
pub fn format_say(speaker: &str, text: &str) -> String {
    tr!("social.say.third_party", speaker = speaker, text = text)
}

/// Format a yell message broadcast to an area.
///
/// `text` is escaped: these still build their string with `format!` rather than
/// going through the catalog, so the escaping `tr` performs does not apply and
/// has to be done here. Once these become channel configuration they inherit it.
pub fn format_yell(speaker: &str, text: &str) -> String {
    format!("{} yells, '{}'\n", speaker, escape_codes(text))
}

/// Format an OOC message broadcast globally.
///
/// See [`format_yell`] for why `text` is escaped here.
pub fn format_ooc(speaker: &str, text: &str) -> String {
    format!("[OOC] {}: {}\n", speaker, escape_codes(text))
}

/// Render an admin `gecho`. `sender: Some(name)` attributes it (`Name> text`)
/// for other admins; `None` yields the raw text (sender + non-admins). Text is
/// escaped so a broadcast can't inject colour codes.
pub fn format_gecho(sender: Option<&str>, text: &str) -> String {
    match sender {
        Some(name) => format!("{}> {}\n", name, escape_codes(text)),
        None => format!("{}\n", escape_codes(text)),
    }
}

/// Format a movement broadcast.
pub fn format_move(actor: &str, direction: &str, leaving: bool) -> String {
    if leaving {
        format!("{} leaves {}.\n", actor, direction)
    } else {
        format!("{} arrives.\n", actor)
    }
}

/// Format the who list.
#[allow(dead_code)]
pub fn format_who_list(players: &[String]) -> String {
    if players.is_empty() {
        "No other players online.\n".into()
    } else {
        let mut out = format!("Players online ({}):\n", players.len());
        for name in players {
            out.push_str(&format!("  {}\n", name));
        }
        out
    }
}

/// Format the where list (same-area players with room names).
#[allow(dead_code)]
pub fn format_where_list(entries: &[(String, String)]) -> String {
    if entries.is_empty() {
        "No other players in this area.\n".into()
    } else {
        let mut out = "Players in your area:\n".to_string();
        for (name, room) in entries {
            out.push_str(&format!("  {} in [{}]\n", name, room));
        }
        out
    }
}

/// Format the `areas` list: each area's slug and display name, already sorted.
#[allow(dead_code)]
pub fn format_areas_list(entries: &[(String, String)]) -> String {
    if entries.is_empty() {
        return "No areas exist.\n".into();
    }
    let mut out = format!("Areas ({}):\n", entries.len());
    for (slug, name) in entries {
        out.push_str(&format!("  {} — {}\n", slug, name));
    }
    out
}

#[allow(dead_code)]
/// Format the command list.
pub fn format_commands() -> String {
    let cmds = [
        "look [target]       — Look at the room or a specific target",
        "l [target]          — Shortcut for look",
        "say <text>          — Speak to everyone in the room",
        "'<text>             — Shortcut for say",
        "yell <text>         — Shout to everyone in the area",
        "ooc <text>          — Out-of-character global chat",
        "tell <who> <text>   — Private message a player (alias: whisper)",
        "reply <text>        — Reply to the last player who whispered you",
        "north / n           — Move north",
        "east / e            — Move east",
        "south / s           — Move south",
        "west / w            — Move west",
        "up / u              — Move up",
        "down / d            — Move down",
        "who                 — List players online",
        "where               — Show who's in your area",
        "areas               — List all areas in the world",
        "commands / help     — Show this list",
        "quit / exit         — Disconnect from the game",
    ];
    let mut out = "Available commands:\n".to_string();
    for cmd in &cmds {
        out.push_str(&format!("  {}\n", cmd));
    }
    out
}

/// Format the MOTD.
pub fn format_motd() -> String {
    include_str!("../../../assets/motd.txt").to_string()
}

/// One row of a creation menu: a display `name`, an identifying `slug` (matched
/// as a prefix), and an optional one-line `description`.
pub struct MenuItem<'a> {
    pub name: &'a str,
    pub slug: &'a str,
    pub description: &'a str,
}

/// Render a numbered creation menu: a `title`, one `N. Name - description` line
/// per item (the `- description` omitted when empty), and a trailing `prompt`.
pub fn format_selection_menu(title: &str, items: &[MenuItem], prompt: &str) -> String {
    let mut out = format!("[ {title} ]\n");
    for (i, item) in items.iter().enumerate() {
        if item.description.is_empty() {
            out.push_str(&format!("{}. {}\n", i + 1, item.name));
        } else {
            out.push_str(&format!(
                "{}. {} - {}\n",
                i + 1,
                item.name,
                item.description
            ));
        }
    }
    out.push('\n');
    out.push_str(prompt);
    out
}

/// Resolve a line of menu input against `items`, returning the 0-based position
/// of the chosen row. Accepts either a 1-based index or a case-insensitive
/// prefix of a row's name or slug (first match wins). Returns `None` on blank,
/// out-of-range, or unmatched input, so the caller re-prompts without advancing.
pub fn parse_menu_choice(input: &str, items: &[MenuItem]) -> Option<usize> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    if let Ok(idx) = input.parse::<usize>() {
        // `then` (lazy) not `then_some`: `idx - 1` would underflow for idx == 0.
        return (idx >= 1 && idx <= items.len()).then(|| idx - 1);
    }
    let lower = input.to_lowercase();
    items.iter().position(|item| {
        item.name.to_lowercase().starts_with(&lower) || item.slug.to_lowercase().starts_with(&lower)
    })
}

/// Format a linkdead announce.
pub fn format_linkdead(name: &str, reconnecting: bool) -> String {
    if reconnecting {
        format!("{} has reconnected.\n", name)
    } else {
        format!("{} has gone linkdead.\n", name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_areas_list ────────────────────────────────────────

    #[test]
    fn areas_list_empty() {
        assert_eq!(format_areas_list(&[]), "No areas exist.\n");
    }

    #[test]
    fn areas_list_lists_slug_and_name() {
        let entries = vec![
            ("haven".to_string(), "Haven".to_string()),
            ("swamp".to_string(), "Southern Swamp".to_string()),
        ];
        let got = format_areas_list(&entries);
        assert!(got.starts_with("Areas (2):\n"));
        assert!(got.contains("  haven — Haven\n"));
        assert!(got.contains("  swamp — Southern Swamp\n"));
    }

    // ── room_title ───────────────────────────────────────────────

    #[test]
    fn room_title_plain_without_debug() {
        assert_eq!(room_title("Town Square", None), "Town Square");
    }

    #[test]
    fn room_title_shows_ids_for_admin() {
        let got = room_title(
            "Town Square",
            Some(RoomDebugIds {
                entity: 42,
                grim: "abc-123",
                slug: "town-square",
            }),
        );
        assert_eq!(got, "Town Square (entity:42 grim:abc-123 slug:town-square)");
    }

    // ── format_room ──────────────────────────────────────────────

    #[test]
    fn room_with_exits_and_occupants() {
        let exits = vec!["north".into(), "east".into()];
        let occs = vec!["Alice".into(), "Bob".into()];
        let got = format_room("The Tavern", "A warm room.", &exits, &occs);
        assert!(got.starts_with("The Tavern\nA warm room."));
        assert!(got.contains("Exits: north, east"));
        assert!(got.contains("Also here: Alice, Bob"));
        assert!(got.ends_with("\n"));
    }

    #[test]
    fn room_no_exits() {
        let got = format_room("Void", "Empty.", &[], &["Guard".into()]);
        assert!(got.starts_with("Void\nEmpty."));
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
        assert_eq!(got, "Empty\nNothing.\n");
    }

    // ── format_entity ────────────────────────────────────────────

    #[test]
    fn entity_basic() {
        assert_eq!(
            format_entity("Sword", "A rusty blade."),
            "Sword\nA rusty blade.\n"
        );
    }

    #[test]
    fn entity_empty_desc() {
        assert_eq!(format_entity("Nothing", ""), "Nothing\n\n");
    }

    // ── format_say ───────────────────────────────────────────────

    #[test]
    fn say_basic() {
        assert_eq!(
            format_say("Alice", "hello there"),
            "@xf0fAlice says @r'@x909hello there@r'\n"
        );
    }
    #[test]
    fn say_empty_text() {
        assert_eq!(format_say("Bob", ""), "@xf0fBob says @r'@x909@r'\n");
    }

    /// `say {RHELLO` used to turn the room's text red. Spoken text is data.
    #[test]
    fn say_does_not_let_speech_inject_colour() {
        let out = format_say("Alice", "{RHELLO");
        let rendered = grim_color::ansi(&grim_color::convert_16color(&out));
        assert!(
            rendered.contains("{RHELLO"),
            "spoken markup must render literally: {rendered:?}"
        );
    }

    /// `yell` and `ooc` build their string with `format!`, not the catalog, so
    /// they need escaping of their own — verify they got it.
    #[test]
    fn yell_and_ooc_do_not_let_speech_inject_colour() {
        for out in [
            format_yell("Alice", "{RHELLO"),
            format_ooc("Alice", "{RHELLO"),
        ] {
            let rendered = grim_color::ansi(&grim_color::convert_16color(&out));
            assert!(
                rendered.contains("{RHELLO"),
                "spoken markup must render literally: {rendered:?}"
            );
            assert!(
                !rendered.contains('\x1b'),
                "no colour emitted: {rendered:?}"
            );
        }
    }

    /// Names cannot currently carry markup — `validate_character_name` allows
    /// only alphanumerics, spaces, hyphens and apostrophes. This guards the
    /// formatter anyway, so a future relaxation of that rule fails loudly here
    /// rather than quietly becoming an injection.
    #[test]
    fn say_does_not_let_a_name_inject_colour() {
        let out = format_say("@xf00Alice", "hi");
        let rendered = grim_color::ansi(&grim_color::convert_16color(&out));
        assert!(
            rendered.contains("@xf00Alice"),
            "name markup must render literally: {rendered:?}"
        );
    }

    // ── format_yell ──────────────────────────────────────────────

    #[test]
    fn yell_basic() {
        assert_eq!(
            format_yell("Guard", "intruder"),
            "Guard yells, 'intruder'\n"
        );
    }

    #[test]
    fn yell_empty_text() {
        assert_eq!(format_yell("Echo", ""), "Echo yells, ''\n");
    }

    // ── format_ooc ───────────────────────────────────────────────

    #[test]
    fn ooc_basic() {
        assert_eq!(
            format_ooc("Alice", "anyone at the tavern?"),
            "[OOC] Alice: anyone at the tavern?\n"
        );
    }

    #[test]
    fn ooc_empty_text() {
        assert_eq!(format_ooc("Bob", ""), "[OOC] Bob: \n");
    }

    // ── format_move ──────────────────────────────────────────────

    #[test]
    fn move_leaving() {
        assert_eq!(format_move("Alice", "north", true), "Alice leaves north.\n");
    }

    #[test]
    fn move_arriving() {
        assert_eq!(format_move("Bob", "east", false), "Bob arrives.\n");
    }

    // ── format_who_list ──────────────────────────────────────────

    #[test]
    fn who_empty() {
        assert_eq!(format_who_list(&[]), "No other players online.\n");
    }

    #[test]
    fn who_populated() {
        let players = vec!["Alice".into(), "Bob".into(), "Charlie".into()];
        let got = format_who_list(&players);
        assert!(got.starts_with("Players online (3):\n"));
        assert!(got.contains("  Alice\n"));
        assert!(got.contains("  Bob\n"));
        assert!(got.contains("  Charlie\n"));
    }

    // ── format_where_list ────────────────────────────────────────

    #[test]
    fn where_empty() {
        assert_eq!(format_where_list(&[]), "No other players in this area.\n");
    }

    #[test]
    fn where_populated() {
        let entries = vec![
            ("Alice".into(), "Tavern".into()),
            ("Bob".into(), "Garden".into()),
        ];
        let got = format_where_list(&entries);
        assert!(got.starts_with("Players in your area:\n"));
        assert!(got.contains("  Alice in [Tavern]\n"));
        assert!(got.contains("  Bob in [Garden]\n"));
    }

    // ── format_commands ──────────────────────────────────────────

    #[test]
    fn commands_contains_known_entries() {
        let got = format_commands();
        assert!(got.starts_with("Available commands:\n"));
        assert!(got.contains("say <text>"));
        assert!(got.contains("who"));
        assert!(got.contains("quit / exit"));
        assert!(got.ends_with("\n"));
    }

    // ── format_motd ──────────────────────────────────────────────

    #[test]
    fn motd_non_empty() {
        let got = format_motd();
        assert!(!got.is_empty());
    }

    // ── format_linkdead ──────────────────────────────────────────

    #[test]
    fn linkdead_gone() {
        assert_eq!(
            format_linkdead("Alice", false),
            "Alice has gone linkdead.\n"
        );
    }

    #[test]
    fn linkdead_reconnected() {
        assert_eq!(format_linkdead("Bob", true), "Bob has reconnected.\n");
    }

    // ── format_selection_menu ────────────────────────────────────

    fn items() -> Vec<MenuItem<'static>> {
        vec![
            MenuItem {
                name: "Warrior",
                slug: "warrior",
                description: "Hits things.",
            },
            MenuItem {
                name: "Mage",
                slug: "mage",
                description: "",
            },
        ]
    }

    #[test]
    fn selection_menu_numbers_and_prompts() {
        let got = format_selection_menu("Class", &items(), "Pick: ");
        assert!(got.starts_with("[ Class ]\n"));
        assert!(got.contains("1. Warrior - Hits things.\n"));
        // Empty description omits the dash.
        assert!(got.contains("2. Mage\n"));
        assert!(got.ends_with("Pick: "));
    }

    // ── parse_menu_choice ────────────────────────────────────────

    #[test]
    fn parse_choice_by_index() {
        assert_eq!(parse_menu_choice("1", &items()), Some(0));
        assert_eq!(parse_menu_choice("2", &items()), Some(1));
    }

    #[test]
    fn parse_choice_index_out_of_range_or_zero() {
        assert_eq!(parse_menu_choice("0", &items()), None);
        assert_eq!(parse_menu_choice("3", &items()), None);
    }

    #[test]
    fn parse_choice_by_name_prefix_case_insensitive() {
        assert_eq!(parse_menu_choice("war", &items()), Some(0));
        assert_eq!(parse_menu_choice("MAGE", &items()), Some(1));
    }

    #[test]
    fn parse_choice_by_slug_prefix() {
        assert_eq!(parse_menu_choice("warr", &items()), Some(0));
    }

    #[test]
    fn parse_choice_blank_or_unmatched_is_none() {
        assert_eq!(parse_menu_choice("", &items()), None);
        assert_eq!(parse_menu_choice("   ", &items()), None);
        assert_eq!(parse_menu_choice("cleric", &items()), None);
    }
}
