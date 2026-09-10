//! Cardinal direction verbs: `north|east|south|west|up|down` (and their
//! single-letter prefixes via the registry's priority matching).
//!
//! Registration lives here — like the warned-countdown verbs in
//! `countdown.rs` — to hold `parser.rs` under its size cap; parse-level
//! tests stay in `parser.rs` with the rest.

use grim_command::CommandRegistry;
use grim_core::cardinal::Cardinal;
use grim_core::events::Command;

/// Register the six direction verbs. Called last in `build_registry`, so
/// each new command front-loads above the last and single-character input
/// (`n`/`e`/`s`/`w`/`u`/`d`) reaches movement first.
pub(crate) fn register(r: &mut CommandRegistry<Command>) {
    r.register("north", |_| {
        Some(Command::Move {
            direction: Cardinal::North,
        })
    });
    r.register("east", |_| {
        Some(Command::Move {
            direction: Cardinal::East,
        })
    });
    r.register("south", |_| {
        Some(Command::Move {
            direction: Cardinal::South,
        })
    });
    r.register("west", |_| {
        Some(Command::Move {
            direction: Cardinal::West,
        })
    });
    r.register("up", |_| {
        Some(Command::Move {
            direction: Cardinal::Up,
        })
    });
    r.register("down", |_| {
        Some(Command::Move {
            direction: Cardinal::Down,
        })
    });
}
