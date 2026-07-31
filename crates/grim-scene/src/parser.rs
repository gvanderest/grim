use bevy::log::warn;
use grim_command::CommandRegistry;
use grim_engine_types::events::Command;

/// Parse a raw input line into a Command using `registry`.
///
/// Prefix matching resolves by priority (see `grim-command`), so single-letter
/// direction shortcuts (n/e/s/w/u/d) work when north/east/... are registered.
pub fn parse_command(registry: &CommandRegistry<Command>, input: &str) -> Option<Command> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (word, rest) = trimmed.split_once(' ').unwrap_or((trimmed, ""));
    registry.resolve(word, rest.trim())
}

/// Build the command registry and log any contested prefixes once, so a
/// silently-shadowed abbreviation surfaces at startup rather than as a confused
/// player. The scene plugin inserts the result as a Bevy resource.
pub fn command_registry() -> CommandRegistry<Command> {
    let r = build_registry();
    for contest in r.contested_prefixes() {
        warn!(
            "command prefix '{}' resolves to '{}', shadowing {}",
            contest.prefix,
            contest.winner,
            contest.shadowed.join(", ")
        );
    }
    r
}

fn build_registry() -> CommandRegistry<Command> {
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
    r.register("areas", |_| Some(Command::Areas));
    r.register("commands", |_| Some(Command::Commands));
    r.register("help", |_| Some(Command::Commands));
    r.register("quit", |_| Some(Command::Quit));
    r.register("exit", |_| Some(Command::Quit));

    // ── Admin ────────────────────────────────────────────────────
    // `shutdown [seconds]` — defaults to 30s when no/invalid count given.
    // Admin-gated at dispatch (grim::plugins::ShutdownPlugin), not here.
    r.register("shutdown", |rest| {
        let seconds = rest.trim().parse::<u64>().unwrap_or(30);
        Some(Command::Shutdown { seconds })
    });
    // `goto <address>` — admin-gated + masked at dispatch (see grim-scene
    // dispatcher). Rejected with no argument so a bare `goto` is unknown.
    r.register("goto", |rest| {
        let target = rest.trim();
        (!target.is_empty()).then(|| Command::Goto {
            target: target.to_string(),
        })
    });

    // ── Cardinal directions (last = highest priority for single-char) ─
    r.register("north", |_| {
        Some(Command::Move {
            direction: grim_engine_types::cardinal::Cardinal::North,
        })
    });
    r.register("east", |_| {
        Some(Command::Move {
            direction: grim_engine_types::cardinal::Cardinal::East,
        })
    });
    r.register("south", |_| {
        Some(Command::Move {
            direction: grim_engine_types::cardinal::Cardinal::South,
        })
    });
    r.register("west", |_| {
        Some(Command::Move {
            direction: grim_engine_types::cardinal::Cardinal::West,
        })
    });
    r.register("up", |_| {
        Some(Command::Move {
            direction: grim_engine_types::cardinal::Cardinal::Up,
        })
    });
    r.register("down", |_| {
        Some(Command::Move {
            direction: grim_engine_types::cardinal::Cardinal::Down,
        })
    });

    r
}

#[cfg(test)]
mod tests {
    use grim_command::CommandRegistry;
    use grim_engine_types::cardinal::Cardinal;
    use grim_engine_types::events::Command;

    use super::{command_registry, parse_command};

    /// Parse against a freshly-built default registry.
    fn parse(input: &str) -> Option<Command> {
        parse_command(&command_registry(), input)
    }

    // ── Cardinal directions ──────────────────────────────────────
    #[test]
    fn test_north() {
        assert_eq!(
            parse("n"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            parse("north"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
    }

    #[test]
    fn test_east() {
        assert_eq!(
            parse("e"),
            Some(Command::Move {
                direction: Cardinal::East
            })
        );
        assert_eq!(
            parse("east"),
            Some(Command::Move {
                direction: Cardinal::East
            })
        );
    }

    #[test]
    fn test_south() {
        assert_eq!(
            parse("s"),
            Some(Command::Move {
                direction: Cardinal::South
            })
        );
        assert_eq!(
            parse("south"),
            Some(Command::Move {
                direction: Cardinal::South
            })
        );
    }

    #[test]
    fn test_west() {
        assert_eq!(
            parse("w"),
            Some(Command::Move {
                direction: Cardinal::West
            })
        );
        assert_eq!(
            parse("west"),
            Some(Command::Move {
                direction: Cardinal::West
            })
        );
    }

    #[test]
    fn test_up() {
        assert_eq!(
            parse("u"),
            Some(Command::Move {
                direction: Cardinal::Up
            })
        );
        assert_eq!(
            parse("up"),
            Some(Command::Move {
                direction: Cardinal::Up
            })
        );
    }

    #[test]
    fn test_down() {
        assert_eq!(
            parse("d"),
            Some(Command::Move {
                direction: Cardinal::Down
            })
        );
        assert_eq!(
            parse("down"),
            Some(Command::Move {
                direction: Cardinal::Down
            })
        );
    }

    // ── Social commands ──────────────────────────────────────────
    #[test]
    fn test_say_with_text() {
        assert_eq!(
            parse("say hello there"),
            Some(Command::Say {
                text: "hello there".to_string()
            })
        );
    }

    #[test]
    fn test_say_empty_is_none() {
        assert_eq!(parse("say"), None);
        assert_eq!(parse("say "), None);
    }

    #[test]
    fn test_say_shorthand() {
        assert_eq!(
            parse("' hello"),
            Some(Command::Say {
                text: "hello".to_string()
            })
        );
        assert_eq!(parse("'"), None);
    }

    #[test]
    fn test_yell_with_text() {
        assert_eq!(
            parse("yell fire"),
            Some(Command::Yell {
                text: "fire".to_string()
            })
        );
    }

    #[test]
    fn test_yell_empty_is_none() {
        assert_eq!(parse("yell"), None);
    }

    #[test]
    fn test_ooc_with_text() {
        assert_eq!(
            parse("ooc anyone here?"),
            Some(Command::Ooc {
                text: "anyone here?".to_string()
            })
        );
    }

    #[test]
    fn test_ooc_empty_is_none() {
        assert_eq!(parse("ooc"), None);
    }

    // ── Look ──────────────────────────────────────────────────────
    #[test]
    fn test_look_without_target() {
        assert_eq!(parse("look"), Some(Command::Look { target: None }));
        assert_eq!(parse("l"), Some(Command::Look { target: None }));
    }

    #[test]
    fn test_look_with_target() {
        assert_eq!(
            parse("look statue"),
            Some(Command::Look {
                target: Some("statue".to_string())
            })
        );
        assert_eq!(
            parse("l statue"),
            Some(Command::Look {
                target: Some("statue".to_string())
            })
        );
    }

    // ── Informational ─────────────────────────────────────────────
    #[test]
    fn test_who() {
        assert_eq!(parse("who"), Some(Command::Who));
    }

    #[test]
    fn test_where_cmd() {
        assert_eq!(parse("where"), Some(Command::Where));
    }

    #[test]
    fn test_areas_cmd() {
        assert_eq!(parse("areas"), Some(Command::Areas));
    }

    #[test]
    fn test_goto_with_target() {
        assert_eq!(
            parse("goto haven:market-square"),
            Some(Command::Goto {
                target: "haven:market-square".to_string()
            })
        );
    }

    #[test]
    fn test_goto_without_target_is_none() {
        assert_eq!(parse("goto"), None);
        assert_eq!(parse("goto   "), None);
    }

    #[test]
    fn test_commands_and_help() {
        assert_eq!(parse("commands"), Some(Command::Commands));
        assert_eq!(parse("help"), Some(Command::Commands));
    }

    // ── Quit ──────────────────────────────────────────────────────
    #[test]
    fn test_quit_and_exit() {
        assert_eq!(parse("quit"), Some(Command::Quit));
        assert_eq!(parse("exit"), Some(Command::Quit));
    }

    // ── Shutdown (admin; gating happens at dispatch) ───────────────
    #[test]
    fn test_shutdown_with_count() {
        assert_eq!(
            parse("shutdown 30"),
            Some(Command::Shutdown { seconds: 30 })
        );
    }

    #[test]
    fn test_shutdown_defaults_to_30() {
        assert_eq!(parse("shutdown"), Some(Command::Shutdown { seconds: 30 }));
        assert_eq!(
            parse("shutdown abc"),
            Some(Command::Shutdown { seconds: 30 })
        );
    }

    // ── Edge cases ────────────────────────────────────────────────
    #[test]
    fn test_empty_input() {
        assert_eq!(parse(""), None);
    }

    #[test]
    fn test_whitespace_only() {
        assert_eq!(parse("   "), None);
        assert_eq!(parse("\t"), None);
        assert_eq!(parse(" \t "), None);
    }

    #[test]
    fn test_unknown_command() {
        assert_eq!(parse("foobar"), None);
        assert_eq!(parse("xyzzy"), None);
        assert_eq!(parse("123"), None);
    }

    #[test]
    fn test_direction_wins_over_social() {
        // "n" is a prefix of "north", which is registered — direction wins
        assert_eq!(
            parse("n hello"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            parse("s"),
            Some(Command::Move {
                direction: Cardinal::South
            })
        );
    }

    // ── Prefix matching / registration order ────────────────────────────

    #[test]
    fn test_prefix_match_partial() {
        // "no" is prefix of "north" (default registry doesn't include "note")
        assert_eq!(
            parse("no"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        // "so" is prefix of "south"
        assert_eq!(
            parse("so"),
            Some(Command::Move {
                direction: Cardinal::South
            })
        );
        // "ea" is prefix of "east"
        assert_eq!(
            parse("ea"),
            Some(Command::Move {
                direction: Cardinal::East
            })
        );
        // "we" is prefix of "west"
        assert_eq!(
            parse("we"),
            Some(Command::Move {
                direction: Cardinal::West
            })
        );
        // "do" is prefix of "down"
        assert_eq!(
            parse("do"),
            Some(Command::Move {
                direction: Cardinal::Down
            })
        );
    }

    #[test]
    fn test_case_insensitivity() {
        assert_eq!(
            parse("NORTH"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            parse("N"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            parse("NoRtH"),
            Some(Command::Move {
                direction: Cardinal::North
            })
        );
        assert_eq!(
            parse("SAY Hello"),
            Some(Command::Say {
                text: "Hello".to_string()
            })
        );
        assert_eq!(parse("LOOK"), Some(Command::Look { target: None }));
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
