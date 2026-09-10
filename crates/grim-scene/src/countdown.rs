//! Warned-countdown admin verbs: `shutdown|reboot|copyover [seconds]`.
//!
//! All three warn like `shutdown`; at expiry the shared countdown halts,
//! cold-restarts, or hands off (see `grim-world`'s tick). Registration lives
//! here — factories plus the one `register` entry — to hold `parser.rs`
//! under its size cap; parse-level tests stay in `parser.rs` with the rest.

use grim_command::CommandRegistry;
use grim_core::events::Command;

/// Parse a `[seconds]` countdown argument (30s on missing/garbage count).
fn countdown_secs(rest: &str) -> u64 {
    rest.trim().parse::<u64>().unwrap_or(30)
}

fn shutdown(rest: &str) -> Option<Command> {
    Some(Command::Shutdown {
        seconds: countdown_secs(rest),
    })
}

fn reboot(rest: &str) -> Option<Command> {
    Some(Command::Reboot {
        seconds: countdown_secs(rest),
    })
}

fn copyover(rest: &str) -> Option<Command> {
    Some(Command::Copyover {
        seconds: countdown_secs(rest),
    })
}

/// Register the three countdown verbs, parked behind older prefixes so `r`
/// still reaches `reply` and `c` still reaches `commands` (exact words
/// always match first). Admin-gated at dispatch, not here.
pub(crate) fn register(r: &mut CommandRegistry<Command>) {
    r.register("shutdown", shutdown);
    r.register("reboot", reboot);
    r.register("copyover", copyover);
    r.deprioritize("reboot");
    r.deprioritize("copyover");
}
