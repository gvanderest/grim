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
        "u" | "up" => Some(Command::Move { direction: Cardinal::Up }),
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