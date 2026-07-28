use grim::cardinal::Cardinal;
use grim::events::Command;

/// Parse user input into a Command.
///
/// Cardinal directions (n/e/s/w/u/d) are checked first for highest priority
/// so single-letter direction shortcuts always win over other commands.
pub fn parse_command(input: &str) -> Option<Command> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (first, rest) = trimmed.split_once(' ').unwrap_or((trimmed, ""));
    let rest = rest.trim();

    match first.to_lowercase().as_str() {
        // Cardinals (highest priority — matched before any other `n`/`e`/etc.)
        "n" | "north" => Some(Command::Move {
            direction: Cardinal::North,
        }),
        "e" | "east" => Some(Command::Move {
            direction: Cardinal::East,
        }),
        "s" | "south" => Some(Command::Move {
            direction: Cardinal::South,
        }),
        "w" | "west" => Some(Command::Move {
            direction: Cardinal::West,
        }),
        "u" | "up" => Some(Command::Move {
            direction: Cardinal::Up,
        }),
        "d" | "down" => Some(Command::Move {
            direction: Cardinal::Down,
        }),

        // Social channels
        "say" | "'" => {
            if rest.is_empty() {
                return None;
            }
            Some(Command::Say {
                text: rest.to_string(),
            })
        }
        "yell" => {
            if rest.is_empty() {
                return None;
            }
            Some(Command::Yell {
                text: rest.to_string(),
            })
        }
        "ooc" => {
            if rest.is_empty() {
                return None;
            }
            Some(Command::Ooc {
                text: rest.to_string(),
            })
        }

        // Game actions
        "look" | "l" => Some(Command::Look {
            target: if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            },
        }),
        "who" => Some(Command::Who),
        "where" => Some(Command::Where),
        "commands" | "help" => Some(Command::Commands),
        "quit" | "exit" => Some(Command::Quit),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use grim::cardinal::Cardinal;
    use grim::events::Command;

    use super::parse_command;

    // ── Cardinal directions ──────────────────────────────────────
    #[test]
    fn test_north() {
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
        assert_eq!(
            parse_command("say hello there"),
            Some(Command::Say {
                text: "hello there".to_string()
            })
        );
    }

    #[test]
    fn test_say_empty_is_none() {
        assert_eq!(parse_command("say"), None);
        assert_eq!(parse_command("say "), None);
    }

    #[test]
    fn test_say_shorthand() {
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
        assert_eq!(
            parse_command("yell fire"),
            Some(Command::Yell {
                text: "fire".to_string()
            })
        );
    }

    #[test]
    fn test_yell_empty_is_none() {
        assert_eq!(parse_command("yell"), None);
    }

    #[test]
    fn test_ooc_with_text() {
        assert_eq!(
            parse_command("ooc anyone here?"),
            Some(Command::Ooc {
                text: "anyone here?".to_string()
            })
        );
    }

    #[test]
    fn test_ooc_empty_is_none() {
        assert_eq!(parse_command("ooc"), None);
    }

    // ── Look ──────────────────────────────────────────────────────
    #[test]
    fn test_look_without_target() {
        assert_eq!(parse_command("look"), Some(Command::Look { target: None }));
        assert_eq!(parse_command("l"), Some(Command::Look { target: None }));
    }

    #[test]
    fn test_look_with_target() {
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
        assert_eq!(parse_command("who"), Some(Command::Who));
    }

    #[test]
    fn test_where_cmd() {
        assert_eq!(parse_command("where"), Some(Command::Where));
    }

    #[test]
    fn test_commands_and_help() {
        assert_eq!(parse_command("commands"), Some(Command::Commands));
        assert_eq!(parse_command("help"), Some(Command::Commands));
    }

    // ── Quit ──────────────────────────────────────────────────────
    #[test]
    fn test_quit_and_exit() {
        assert_eq!(parse_command("quit"), Some(Command::Quit));
        assert_eq!(parse_command("exit"), Some(Command::Quit));
    }

    // ── Edge cases ────────────────────────────────────────────────
    #[test]
    fn test_empty_input() {
        assert_eq!(parse_command(""), None);
    }

    #[test]
    fn test_whitespace_only() {
        assert_eq!(parse_command("   "), None);
        assert_eq!(parse_command("\t"), None);
        assert_eq!(parse_command(" \t "), None);
    }

    #[test]
    fn test_unknown_command() {
        assert_eq!(parse_command("foobar"), None);
        assert_eq!(parse_command("xyzzy"), None);
        assert_eq!(parse_command("123"), None);
    }

    #[test]
    fn test_direction_wins_over_social() {
        // Single-letter direction shortcuts always parse as Move, never as social.
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
}
