use std::sync::OnceLock;

use grim::command_registry::CommandRegistry;
use grim::events::Command;

/// Global command registry, initialized once at plugin startup.
static REGISTRY: OnceLock<CommandRegistry> = OnceLock::new();

/// Parse user input into a Command using the global command registry.
///
/// The registry handles prefix matching with "last registered wins" priority,
/// so single-letter direction shortcuts (n/e/s/w/u/d) work naturally when
/// "north"/"east"/etc. are registered.
///
/// # Panics
///
/// Panics if called before [`init_registry`] has been invoked.
pub fn parse_command(input: &str) -> Option<Command> {
    let registry = REGISTRY
        .get()
        .expect("CommandRegistry not initialized — call init_registry() before parse_command()");

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (word, rest) = trimmed.split_once(' ').unwrap_or((trimmed, ""));
    let rest = rest.trim();

    registry.resolve(word, rest)
}

/// Initialize the global command registry with all game commands.
///
/// Safe to call multiple times — only the first call takes effect.
pub fn init_registry() -> &'static CommandRegistry {
    REGISTRY.get_or_init(build_registry)
}

fn build_registry() -> CommandRegistry {
    let mut r = CommandRegistry::new();

    // ── Social commands ──────────────────────────────────────────
    r.register("say", |rest| {
        if rest.is_empty() {
            None
        } else {
            Some(Command::Say {
                text: rest.to_string(),
            })
        }
    });
    r.register("'", |rest| {
        if rest.is_empty() {
            None
        } else {
            Some(Command::Say {
                text: rest.to_string(),
            })
        }
    });
    r.register("yell", |rest| {
        if rest.is_empty() {
            None
        } else {
            Some(Command::Yell {
                text: rest.to_string(),
            })
        }
    });
    r.register("ooc", |rest| {
        if rest.is_empty() {
            None
        } else {
            Some(Command::Ooc {
                text: rest.to_string(),
            })
        }
    });

    // ── Game actions ─────────────────────────────────────────────
    r.register("look", |rest| {
        if rest.is_empty() {
            Some(Command::Look { target: None })
        } else {
            Some(Command::Look {
                target: Some(rest.to_string()),
            })
        }
    });
    // 'l' shorthand for look
    r.register("l", |rest| {
        if rest.is_empty() {
            Some(Command::Look { target: None })
        } else {
            Some(Command::Look {
                target: Some(rest.to_string()),
            })
        }
    });
    r.register("who", |_| Some(Command::Who));
    r.register("where", |_| Some(Command::Where));
    r.register("commands", |_| Some(Command::Commands));
    r.register("help", |_| Some(Command::Commands));
    r.register("quit", |_| Some(Command::Quit));
    r.register("exit", |_| Some(Command::Quit));

    // ── Cardinal directions (last = highest priority for single-char) ─
    r.register("north", |_| {
        Some(Command::Move {
            direction: grim::cardinal::Cardinal::North,
        })
    });
    r.register("east", |_| {
        Some(Command::Move {
            direction: grim::cardinal::Cardinal::East,
        })
    });
    r.register("south", |_| {
        Some(Command::Move {
            direction: grim::cardinal::Cardinal::South,
        })
    });
    r.register("west", |_| {
        Some(Command::Move {
            direction: grim::cardinal::Cardinal::West,
        })
    });
    r.register("up", |_| {
        Some(Command::Move {
            direction: grim::cardinal::Cardinal::Up,
        })
    });
    r.register("down", |_| {
        Some(Command::Move {
            direction: grim::cardinal::Cardinal::Down,
        })
    });

    r
}

#[cfg(test)]
mod tests {
    use grim::cardinal::Cardinal;
    use grim::command_registry::CommandRegistry;
    use grim::events::Command;

    use super::{init_registry, parse_command};

    fn setup() {
        init_registry();
    }

    // ── Cardinal directions ──────────────────────────────────────
    #[test]
    fn test_north() {
        setup();
        assert_eq!(
            parse_command("n"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            parse_command("north"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
    }

    #[test]
    fn test_east() {
        setup();
        assert_eq!(
            parse_command("e"),
            Some(Command::Move {
                direction: Cardinal::East
            })
        );
        assert_eq!(
            parse_command("east"),
            Some(Command::Move {
                direction: Cardinal::East
            })
        );
    }

    #[test]
    fn test_south() {
        setup();
        assert_eq!(
            parse_command("s"),
            Some(Command::Move {
                direction: Cardinal::South
            })
        );
        assert_eq!(
            parse_command("south"),
            Some(Command::Move {
                direction: Cardinal::South
            })
        );
    }

    #[test]
    fn test_west() {
        setup();
        assert_eq!(
            parse_command("w"),
            Some(Command::Move {
                direction: Cardinal::West
            })
        );
        assert_eq!(
            parse_command("west"),
            Some(Command::Move {
                direction: Cardinal::West
            })
        );
    }

    #[test]
    fn test_up() {
        setup();
        assert_eq!(
            parse_command("u"),
            Some(Command::Move {
                direction: Cardinal::Up
            })
        );
        assert_eq!(
            parse_command("up"),
            Some(Command::Move {
                direction: Cardinal::Up
            })
        );
    }

    #[test]
    fn test_down() {
        setup();
        assert_eq!(
            parse_command("d"),
            Some(Command::Move {
                direction: Cardinal::Down
            })
        );
        assert_eq!(
            parse_command("down"),
            Some(Command::Move {
                direction: Cardinal::Down
            })
        );
    }

    // ── Social commands ──────────────────────────────────────────
    #[test]
    fn test_say_with_text() {
        setup();
        assert_eq!(
            parse_command("say hello there"),
            Some(Command::Say {
                text: "hello there".to_string()
            })
        );
    }

    #[test]
    fn test_say_empty_is_none() {
        setup();
        assert_eq!(parse_command("say"), None);
        assert_eq!(parse_command("say "), None);
    }

    #[test]
    fn test_say_shorthand() {
        setup();
        assert_eq!(
            parse_command("' hello"),
            Some(Command::Say {
                text: "hello".to_string()
            })
        );
        assert_eq!(parse_command("'"), None);
    }

    #[test]
    fn test_yell_with_text() {
        setup();
        assert_eq!(
            parse_command("yell fire"),
            Some(Command::Yell {
                text: "fire".to_string()
            })
        );
    }

    #[test]
    fn test_yell_empty_is_none() {
        setup();
        assert_eq!(parse_command("yell"), None);
    }

    #[test]
    fn test_ooc_with_text() {
        setup();
        assert_eq!(
            parse_command("ooc anyone here?"),
            Some(Command::Ooc {
                text: "anyone here?".to_string()
            })
        );
    }

    #[test]
    fn test_ooc_empty_is_none() {
        setup();
        assert_eq!(parse_command("ooc"), None);
    }

    // ── Look ──────────────────────────────────────────────────────
    #[test]
    fn test_look_without_target() {
        setup();
        assert_eq!(parse_command("look"), Some(Command::Look { target: None }));
        assert_eq!(parse_command("l"), Some(Command::Look { target: None }));
    }

    #[test]
    fn test_look_with_target() {
        setup();
        assert_eq!(
            parse_command("look statue"),
            Some(Command::Look {
                target: Some("statue".to_string())
            })
        );
        assert_eq!(
            parse_command("l statue"),
            Some(Command::Look {
                target: Some("statue".to_string())
            })
        );
    }

    // ── Informational ─────────────────────────────────────────────
    #[test]
    fn test_who() {
        setup();
        assert_eq!(parse_command("who"), Some(Command::Who));
    }

    #[test]
    fn test_where_cmd() {
        setup();
        assert_eq!(parse_command("where"), Some(Command::Where));
    }

    #[test]
    fn test_commands_and_help() {
        setup();
        assert_eq!(parse_command("commands"), Some(Command::Commands));
        assert_eq!(parse_command("help"), Some(Command::Commands));
    }

    // ── Quit ──────────────────────────────────────────────────────
    #[test]
    fn test_quit_and_exit() {
        setup();
        assert_eq!(parse_command("quit"), Some(Command::Quit));
        assert_eq!(parse_command("exit"), Some(Command::Quit));
    }

    // ── Edge cases ────────────────────────────────────────────────
    #[test]
    fn test_empty_input() {
        setup();
        assert_eq!(parse_command(""), None);
    }

    #[test]
    fn test_whitespace_only() {
        setup();
        assert_eq!(parse_command("   "), None);
        assert_eq!(parse_command("\t"), None);
        assert_eq!(parse_command(" \t "), None);
    }

    #[test]
    fn test_unknown_command() {
        setup();
        assert_eq!(parse_command("foobar"), None);
        assert_eq!(parse_command("xyzzy"), None);
        assert_eq!(parse_command("123"), None);
    }

    #[test]
    fn test_direction_wins_over_social() {
        setup();
        // "n" is a prefix of "north", which is registered — direction wins
        assert_eq!(
            parse_command("n hello"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            parse_command("s"),
            Some(Command::Move {
                direction: Cardinal::South
            })
        );
    }

    // ── Prefix matching / registration order ────────────────────────────

    #[test]
    fn test_prefix_match_partial() {
        setup();
        // "no" is prefix of "north" (default registry doesn't include "note")
        assert_eq!(
            parse_command("no"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        // "so" is prefix of "south"
        assert_eq!(
            parse_command("so"),
            Some(Command::Move {
                direction: Cardinal::South
            })
        );
        // "ea" is prefix of "east"
        assert_eq!(
            parse_command("ea"),
            Some(Command::Move {
                direction: Cardinal::East
            })
        );
        // "we" is prefix of "west"
        assert_eq!(
            parse_command("we"),
            Some(Command::Move {
                direction: Cardinal::West
            })
        );
        // "do" is prefix of "down"
        assert_eq!(
            parse_command("do"),
            Some(Command::Move {
                direction: Cardinal::Down
            })
        );
    }

    #[test]
    fn test_case_insensitivity() {
        setup();
        assert_eq!(
            parse_command("NORTH"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            parse_command("N"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            parse_command("NoRtH"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            parse_command("SAY Hello"),
            Some(Command::Say {
                text: "Hello".to_string()
            })
        );
        assert_eq!(parse_command("LOOK"), Some(Command::Look { target: None }));
    }

    /// Test custom registry with registration-order priority.
    /// Builds its own registry to avoid relying on the global static.
    #[test]
    fn test_custom_registry_order() {
        let mut r = CommandRegistry::new();
        r.register("note", |rest| {
            Some(Command::Say {
                text: rest.to_string(),
            })
        });
        r.register("nordic", |rest| {
            Some(Command::Yell {
                text: rest.to_string(),
            })
        });
        r.register("north", |_: &str| {
            Some(Command::Move {
                direction: Cardinal::North,
            })
        });

        // "n" is prefix of all three; last registered (north) wins
        assert_eq!(
            r.resolve("n", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        // "no" matches note + north; north wins
        assert_eq!(
            r.resolve("no", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        // "nor" matches nordic + north; north wins
        assert_eq!(
            r.resolve("nor", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        // "nort" matches only north
        assert_eq!(
            r.resolve("nort", ""),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        // "nord" matches only nordic
        assert_eq!(
            r.resolve("nord", ""),
            Some(Command::Yell {
                text: "".to_string()
            })
        );
        // "not" matches only note
        assert_eq!(
            r.resolve("not", ""),
            Some(Command::Say {
                text: "".to_string()
            })
        );
    }
}
