use bevy::log::warn;
use grim_command::CommandRegistry;
use grim_core::events::{Command, DescOp};

use crate::split::split_transfer;

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

/// Parse `<target> <message>` for `tell`/`whisper`. Both parts required.
fn parse_tell(rest: &str) -> Option<Command> {
    let (target, text) = rest.split_once(' ')?;
    let (target, text) = (target.trim(), text.trim());
    if target.is_empty() || text.is_empty() {
        return None;
    }
    Some(Command::Tell {
        target: target.to_string(),
        text: text.to_string(),
    })
}

/// Parse `config [key] [value]`: bare lists settings, one word shows one,
/// two words set one. More words are unknown — values are single tokens.
fn parse_config(rest: &str) -> Option<Command> {
    let mut words = rest.split_whitespace();
    let key = words.next();
    let value = words.next();
    if words.next().is_some() {
        return None;
    }
    Some(Command::Config {
        key: key.map(str::to_string),
        value: value.map(str::to_string),
    })
}

#[allow(clippy::too_many_lines)] // reason: flat command-registration list
fn build_registry() -> CommandRegistry<Command> {
    let mut r = CommandRegistry::new();

    // ── Social commands ──────────────────────────────────────────
    r.register("say", |rest| {
        if rest.is_empty() {
            None
        } else {
            Some(Command::Channel {
                channel: "say".to_string(),
                text: rest.to_string(),
            })
        }
    });
    r.register("yell", |rest| {
        if rest.is_empty() {
            None
        } else {
            Some(Command::Channel {
                channel: "yell".to_string(),
                text: rest.to_string(),
            })
        }
    });
    r.register("ooc", |rest| {
        if rest.is_empty() {
            None
        } else {
            Some(Command::Channel {
                channel: "ooc".to_string(),
                text: rest.to_string(),
            })
        }
    });
    // `channel <name> <text>` — a generic command for all configured channels.
    // This allows players to use custom channels added via ChannelRegistry.
    r.register("channel", |rest| {
        let rest = rest.trim();
        let (channel, text) = rest.split_once(' ')?;
        let (channel, text) = (channel.trim(), text.trim());
        if channel.is_empty() || text.is_empty() {
            return None;
        }
        Some(Command::Channel {
            channel: channel.to_string(),
            text: text.to_string(),
        })
    });
    // `tell <target> <message>` (alias: `whisper`). Needs both a target and a
    // non-empty message; a bare `tell` or `tell name` is rejected.
    r.register("tell", parse_tell);
    r.register("whisper", parse_tell);
    // `reply <message>` — to the last player who whispered you.
    r.register("reply", |rest| {
        let text = rest.trim();
        (!text.is_empty()).then(|| Command::Reply {
            text: text.to_string(),
        })
    });
    // `title <text>` sets the WHO title; a bare `title` clears it. Always parses
    // (empty = clear), so it is a recognized command either way.
    r.register("title", |rest| {
        Some(Command::Title {
            text: rest.trim().to_string(),
        })
    });

    // ── Game actions ─────────────────────────────────────────────
    r.register("look", |rest| {
        if rest.is_empty() {
            Some(Command::Look { target: None })
        } else if let Some(inside) = rest.strip_prefix("in ") {
            let container = inside.trim();
            (!container.is_empty()).then(|| Command::LookIn {
                container: container.to_string(),
            })
        } else {
            Some(Command::Look {
                target: Some(rest.to_string()),
            })
        }
    });
    // `desc` views your paragraphs; `desc clear` empties them,
    // `desc + <line>` appends one, `desc -` drops the last, `desc edit`
    // opens the line editor. Anything else (including a bare `+`) is unknown.
    r.register("desc", |rest| {
        let rest = rest.trim();
        if rest.is_empty() {
            Some(Command::Desc { op: DescOp::Show })
        } else if rest.eq_ignore_ascii_case("clear") {
            Some(Command::Desc { op: DescOp::Clear })
        } else if rest.eq_ignore_ascii_case("edit") {
            Some(Command::Desc { op: DescOp::Edit })
        } else if rest == "-" {
            Some(Command::Desc { op: DescOp::Remove })
        } else {
            let line = rest.strip_prefix('+')?.trim();
            (!line.is_empty()).then(|| Command::Desc {
                op: DescOp::Add(line.to_string()),
            })
        }
    });
    // `map` — bare only (`map <anything>` is unknown, like `sockets`). The `m`
    // prefix is uncontested, so this steals no abbreviation.
    r.register("map", |rest| rest.trim().is_empty().then_some(Command::Map));
    r.register("who", |_| Some(Command::Who));
    r.register("wizlist", |_| Some(Command::Wizlist));
    r.register("where", |_| Some(Command::Where));
    r.register("afk", |_| Some(Command::Afk));
    // `finger <name>` — rejected with no argument so a bare `finger` is unknown.
    r.register("finger", |rest| {
        let target = rest.trim();
        (!target.is_empty()).then(|| Command::Finger {
            target: target.to_string(),
        })
    });
    r.register("inventory", |_| Some(Command::Inventory));
    r.register("equipment", |_| Some(Command::Equipment));
    r.register("inv", |_| Some(Command::Inventory));
    r.register("eq", |_| Some(Command::Equipment));
    r.register("areas", |_| Some(Command::Areas));
    // `config [key] [value]` — list settings, cycle one, or set one.
    // Registered BEFORE `commands` so the shared `c`/`co` abbreviations keep
    // reaching the older verb (`con` never matched `commands` — `com` vs
    // `con` — so it and longer prefixes reach `config`); exact words match.
    r.register("config", parse_config);
    r.register("commands", |_| Some(Command::Commands));
    r.register("help", |_| Some(Command::Commands));
    // `sockets` — admin-only, session-local (masked at dispatch, see
    // `handle_ingame`). Takes no argument, so `sockets <anything>` is unknown.
    // Registered before the directions so single-letter/short prefixes still
    // resolve to movement first.
    r.register("sockets", |rest| {
        rest.trim().is_empty().then_some(Command::Sockets)
    });
    // `wiznet [on|off|security|logins]` — admin-only, engine-queued + masked
    // (see `handle_wiznet`). Bare lists; one token sets/toggles; more than
    // one word is unknown.
    r.register("wiznet", |rest| {
        let words: Vec<&str> = rest.split_whitespace().collect();
        match words.as_slice() {
            [] => Some(Command::Wiznet { arg: None }),
            [one] => Some(Command::Wiznet {
                arg: Some(one.to_string()),
            }),
            _ => None,
        }
    });
    // `wiz` is now contested (wizlist vs wiznet): keep it resolving to the
    // older verb so no muscle memory breaks (`wizn…` still reaches wiznet).
    r.prioritize("wizlist");
    r.register("quit", |_| Some(Command::Quit));
    r.register("exit", |_| Some(Command::Quit));
    // `recall` — return to the Town Square. Bare only (`recall <anything>`
    // is unknown, like `map`/`sockets`).
    r.register("recall", |rest| {
        rest.trim().is_empty().then_some(Command::Recall)
    });
    // Bare-only factories reject the rest line, and resolution never falls
    // through to the next prefix candidate — so a new bare verb must sink
    // below the older verbs sharing its prefixes (`reboot`/`copyover` do the
    // same): short `r`/`re` keep reaching `reply`, only `rec…` reaches recall.
    r.deprioritize("recall");
    // `get <keyword>` / `drop <keyword>` — need a target, so a bare `get` or
    // `drop` is unknown. Registered before the admin verbs so the `ge` prefix
    // still resolves to `gecho` (later registrations win prefix ties).
    r.register("get", |rest| {
        let target = rest.trim();
        if target.is_empty() {
            return None;
        }
        // `get <item> <container>`: the container is the LAST token, the
        // item everything before it. Plain `get <target>` stays ground
        // pickup (`all` alone is the ground selector, never a container).
        if let Some((item, container)) = target.rsplit_once(char::is_whitespace) {
            let (item, container) = (item.trim(), container.trim());
            if !item.is_empty() && !container.is_empty() {
                return Some(Command::GetFrom {
                    item: item.to_string(),
                    container: container.to_string(),
                });
            }
        }
        Some(Command::Get {
            target: target.to_string(),
        })
    });
    r.register("put", |rest| {
        let rest = rest.trim();
        let (item, container) = rest.rsplit_once(char::is_whitespace)?;
        let (item, container) = (item.trim(), container.trim());
        (!item.is_empty() && !container.is_empty()).then(|| Command::PutIn {
            item: item.to_string(),
            container: container.to_string(),
        })
    });
    r.register("drop", |rest| {
        let target = rest.trim();
        (!target.is_empty()).then(|| Command::Drop {
            target: target.to_string(),
        })
    });
    // Door verbs (`open|close <direction>`) live in `doors.rs`
    // (factories + priority); register them here with the rest.
    crate::doors::register(&mut r);
    // `give <item> <target>` / `steal <item> <target>` — quote-aware split
    // (`split_transfer`): the item is one token (selector prefixes and
    // `"quoted phrases"` stay glued), the being the rest — except an
    // `all`-headed item, which runs to the last token. Alongside get/drop so
    // the `ge` prefix still reaches `gecho`.
    r.register("give", |rest| {
        let (item, target) = split_transfer(rest)?;
        Some(Command::Give { item, target })
    });
    r.register("steal", |rest| {
        let (item, target) = split_transfer(rest)?;
        Some(Command::Steal { item, target })
    });
    // Combat verbs (`kill`/`flee`/`switch`/`kick`/`cast` + posture). `kill`
    // needs a target; `flee`/`sit`/`sleep`/`stand` are bare; `switch` needs a
    // target; `kick` takes an optional target; `cast` needs the spell slug.
    r.register("kill", |rest| {
        let target = rest.trim();
        (!target.is_empty()).then(|| Command::Kill {
            target: target.to_string(),
        })
    });
    r.register("flee", |rest| {
        rest.trim().is_empty().then_some(Command::Flee)
    });
    r.register("switch", |rest| {
        let target = rest.trim();
        (!target.is_empty()).then(|| Command::Switch {
            target: target.to_string(),
        })
    });
    r.register("kick", |rest| {
        let target = rest.trim();
        Some(Command::Kick {
            target: (!target.is_empty()).then(|| target.to_string()),
        })
    });
    r.register("cast", |rest| {
        let rest = rest.trim();
        let (spell, target) = match rest.split_once(char::is_whitespace) {
            Some((s, t)) => (
                s.trim(),
                (!t.trim().is_empty()).then(|| t.trim().to_string()),
            ),
            None => (rest, None),
        };
        (!spell.is_empty()).then(|| Command::Cast {
            spell: spell.to_string(),
            target,
        })
    });
    r.register("sit", |rest| rest.trim().is_empty().then_some(Command::Sit));
    r.register("sleep", |rest| {
        rest.trim().is_empty().then_some(Command::Sleep)
    });
    r.register("stand", |rest| {
        rest.trim().is_empty().then_some(Command::Stand)
    });
    // `score` — bare only, like `map`/`recall`.
    r.register("score", |rest| {
        rest.trim().is_empty().then_some(Command::Score)
    });
    // New verbs must not steal older abbreviations: `c` stays `commands`,
    // `s` stays game verbs (`say`/`sockets`). Sink the newcomers below the
    // incumbents.
    r.deprioritize("cast");
    r.deprioritize("score");
    r.deprioritize("sit");
    r.deprioritize("sleep");

    // ── Admin ────────────────────────────────────────────────────
    // Warned-countdown verbs (`shutdown|reboot|copyover`) live in
    // `countdown.rs` (factories + priority); register them here with the rest.
    crate::countdown::register(&mut r);
    // `goto <address>` — admin-gated + masked at dispatch (see grim-scene
    // dispatcher). Rejected with no argument so a bare `goto` is unknown.
    r.register("goto", |rest| {
        let target = rest.trim();
        (!target.is_empty()).then(|| Command::Goto {
            target: target.to_string(),
        })
    });
    // `gecho <text>` — admin-gated + masked at dispatch. Rejected with no
    // argument so a bare `gecho` is unknown.
    r.register("gecho", |rest| {
        let text = rest.trim();
        (!text.is_empty()).then(|| Command::Gecho {
            text: text.to_string(),
        })
    });
    // `ban list [type]` / `ban add <type> <pattern>` /
    // `ban remove <type> <pattern>` — admin-gated + masked at dispatch (see
    // `handle_ingame`). Anything else (bare `ban`, unknown type, missing or
    // extra args) is unknown, like the other admin verbs.
    r.register("ban", crate::ban::parse_ban);
    // ── Cardinal directions (last = highest priority for single-char) ─
    crate::directions::register(&mut r);

    // `goto` and `gecho` share the `g` prefix. `register` front-loads priority,
    // so `gecho` (registered later) would otherwise win the bare `g`
    // abbreviation. `goto` predates `gecho` and `g` has long meant goto, so keep
    // it: promote `goto` above `gecho`. Longer prefixes (`ge…`) still reach
    // `gecho` unambiguously.
    r.prioritize("goto");

    r
}
#[cfg(test)]
mod tests {
    use grim_command::CommandRegistry;
    use grim_core::cardinal::Cardinal;
    use grim_core::events::Command;
    use grim_core::events::{BanKind, BanOp, DescOp};

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
            Some(Command::Channel {
                channel: "say".to_string(),
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
    fn test_say_quote_is_not_a_command() {
        // The `'` shorthand is gone: not a registered name, and no command
        // starts with it, so it resolves to nothing.
        assert_eq!(parse("' hello"), None);
        assert_eq!(parse("'"), None);
    }

    #[test]
    fn test_yell_with_text() {
        assert_eq!(
            parse("yell fire"),
            Some(Command::Channel {
                channel: "yell".to_string(),
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
            Some(Command::Channel {
                channel: "ooc".to_string(),
                text: "anyone here?".to_string()
            })
        );
    }

    #[test]
    fn test_ooc_empty_is_none() {
        assert_eq!(parse("ooc"), None);
    }

    #[test]
    fn test_tell_with_target_and_message() {
        assert_eq!(
            parse("tell Wrack hello there"),
            Some(Command::Tell {
                target: "Wrack".to_string(),
                text: "hello there".to_string()
            })
        );
    }

    #[test]
    fn test_whisper_is_alias_for_tell() {
        assert_eq!(
            parse("whisper self hi"),
            Some(Command::Tell {
                target: "self".to_string(),
                text: "hi".to_string()
            })
        );
    }

    #[test]
    fn test_tell_without_message_is_none() {
        assert_eq!(parse("tell"), None);
        assert_eq!(parse("tell Wrack"), None);
        assert_eq!(parse("tell Wrack   "), None);
    }

    // ── Title ─────────────────────────────────────────────────────
    #[test]
    fn test_title_with_text() {
        assert_eq!(
            parse("title the Bold"),
            Some(Command::Title {
                text: "the Bold".to_string()
            })
        );
    }

    #[test]
    fn test_title_bare_clears() {
        // A bare `title` is a recognized command that clears (empty text).
        assert_eq!(
            parse("title"),
            Some(Command::Title {
                text: String::new()
            })
        );
        assert_eq!(
            parse("title   "),
            Some(Command::Title {
                text: String::new()
            })
        );
    }

    // ── Look ──────────────────────────────────────────────────────
    // `l` is not a registered name; it reaches `look` by prefix.
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
    fn test_afk() {
        assert_eq!(parse("afk"), Some(Command::Afk));
    }

    #[test]
    fn test_wizlist() {
        assert_eq!(parse("wizlist"), Some(Command::Wizlist));
    }

    #[test]
    fn test_where_cmd() {
        assert_eq!(parse("where"), Some(Command::Where));
    }

    #[test]
    fn test_finger() {
        assert_eq!(
            parse("finger wrack"),
            Some(Command::Finger {
                target: "wrack".to_string()
            })
        );
        assert_eq!(parse("finger"), None);
        assert_eq!(parse("finger   "), None);
    }

    #[test]
    fn test_desc() {
        assert_eq!(parse("desc"), Some(Command::Desc { op: DescOp::Show }));
        assert_eq!(
            parse("desc clear"),
            Some(Command::Desc { op: DescOp::Clear })
        );
        assert_eq!(
            parse("desc + Hello there."),
            Some(Command::Desc {
                op: DescOp::Add("Hello there.".to_string())
            })
        );
        assert_eq!(parse("desc -"), Some(Command::Desc { op: DescOp::Remove }));
        assert_eq!(parse("desc edit"), Some(Command::Desc { op: DescOp::Edit }));
        assert_eq!(parse("desc EDIT"), Some(Command::Desc { op: DescOp::Edit }));
        assert_eq!(parse("desc +"), None);
        assert_eq!(parse("desc +   "), None);
        assert_eq!(parse("desc bogus"), None);
    }

    #[test]
    fn test_wiznet() {
        assert_eq!(parse("wiznet"), Some(Command::Wiznet { arg: None }));
        assert_eq!(
            parse("wiz"),
            Some(Command::Wizlist),
            "contested prefix stays with the older verb"
        );
        assert_eq!(
            parse("wizn"),
            Some(Command::Wiznet { arg: None }),
            "unambiguous prefix resolves"
        );
        assert_eq!(
            parse("wiznet security"),
            Some(Command::Wiznet {
                arg: Some("security".into())
            })
        );
        assert_eq!(
            parse("WIZNET OFF"),
            Some(Command::Wiznet {
                arg: Some("OFF".into())
            }),
            "arg case preserved for the handler"
        );
        assert_eq!(parse("wiznet too many"), None);
    }

    #[test]
    fn test_sockets() {
        assert_eq!(parse("sockets"), Some(Command::Sockets));
        // Unambiguous prefix resolves; an argument rejects (bare verb only).
        assert_eq!(parse("sock"), Some(Command::Sockets));
        assert_eq!(parse("sockets foo"), None);
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
    fn test_recall() {
        assert_eq!(parse("recall"), Some(Command::Recall));
        // Unambiguous prefix resolves; an argument rejects (bare verb only).
        assert_eq!(parse("rec"), Some(Command::Recall));
        assert_eq!(parse("recall tavern"), None);
        // The bare-only factory rejects, with no fall-through — so recall is
        // deprioritized and short `re` still reaches the older `reply`.
        assert_eq!(
            parse("re hello"),
            Some(Command::Reply {
                text: "hello".into()
            })
        );
    }

    #[test]
    fn test_g_prefix_resolves_to_goto_not_gecho() {
        // `goto` and `gecho` share the `g` prefix; `goto` is prioritized so the
        // bare `g` abbreviation keeps meaning goto (regression guard).
        assert_eq!(
            parse("g haven:market-square"),
            Some(Command::Goto {
                target: "haven:market-square".to_string()
            })
        );
    }

    #[test]
    fn test_ge_prefix_still_resolves_to_gecho() {
        assert_eq!(
            parse("ge hello world"),
            Some(Command::Gecho {
                text: "hello world".to_string()
            })
        );
    }

    #[test]
    fn test_gecho_with_text() {
        assert_eq!(
            parse("gecho server reboot soon"),
            Some(Command::Gecho {
                text: "server reboot soon".to_string()
            })
        );
    }

    #[test]
    fn test_gecho_without_text_is_none() {
        assert_eq!(parse("gecho"), None);
        assert_eq!(parse("gecho   "), None);
    }
    #[test]
    fn test_ban_list() {
        assert_eq!(
            parse("ban list"),
            Some(Command::Ban {
                op: BanOp::List { filter: None }
            })
        );
        assert_eq!(
            parse("ban list IP"),
            Some(Command::Ban {
                op: BanOp::List {
                    filter: Some(BanKind::Ip)
                }
            })
        );
        assert_eq!(
            parse("ban list account"),
            Some(Command::Ban {
                op: BanOp::List {
                    filter: Some(BanKind::Account)
                }
            })
        );
    }

    #[test]
    fn test_ban_add_remove() {
        assert_eq!(
            parse("ban add ip 127.0.*"),
            Some(Command::Ban {
                op: BanOp::Add {
                    kind: BanKind::Ip,
                    pattern: "127.0.*".to_string()
                }
            })
        );
        assert_eq!(
            parse("BAN REMOVE Character Villain"),
            Some(Command::Ban {
                op: BanOp::Remove {
                    kind: BanKind::Character,
                    pattern: "Villain".to_string()
                }
            })
        );
    }

    #[test]
    fn test_ban_malformed_is_none() {
        assert_eq!(parse("ban"), None);
        assert_eq!(parse("ban frobnicate"), None);
        assert_eq!(parse("ban list email"), None);
        assert_eq!(parse("ban list ip extra"), None);
        assert_eq!(parse("ban add ip"), None);
        assert_eq!(parse("ban add email x@y.z"), None);
        assert_eq!(parse("ban add ip 1.2.3.4 extra"), None);
        assert_eq!(parse("ban remove character"), None);
    }

    #[test]
    fn test_commands_and_help() {
        assert_eq!(parse("commands"), Some(Command::Commands));
        assert_eq!(parse("help"), Some(Command::Commands));
    }
    #[test]
    fn test_inventory_and_equipment() {
        assert_eq!(parse("inventory"), Some(Command::Inventory));
        assert_eq!(parse("inv"), Some(Command::Inventory));
        assert_eq!(parse("equipment"), Some(Command::Equipment));
        assert_eq!(parse("eq"), Some(Command::Equipment));
        // Single `e` still moves east; `eq` reaches equipment unambiguously.
        assert_eq!(
            parse("e"),
            Some(Command::Move {
                direction: Cardinal::East
            })
        );
        assert_eq!(parse("eq"), Some(Command::Equipment));
    }

    // ── Get / drop ──────────────────────────────────────────────────
    #[test]
    fn test_get_and_drop_need_targets() {
        assert_eq!(
            parse("get lantern"),
            Some(Command::Get {
                target: "lantern".into()
            })
        );
        assert_eq!(
            parse("drop lantern"),
            Some(Command::Drop {
                target: "lantern".into()
            })
        );
        assert_eq!(parse("get"), None);
        assert_eq!(parse("drop"), None);
    }

    #[test]
    fn test_ge_still_reaches_gecho_and_g_reaches_goto() {
        // `get` registers before the admin verbs, so the `ge` prefix keeps
        // resolving to `gecho` (later registrations win prefix ties).
        assert_eq!(
            parse("gecho hello"),
            Some(Command::Gecho {
                text: "hello".into()
            })
        );
        assert_eq!(
            parse("ge hello"),
            Some(Command::Gecho {
                text: "hello".into()
            })
        );
        assert_eq!(
            parse("g square"),
            Some(Command::Goto {
                target: "square".into()
            })
        );
    }

    // ── Give / steal ────────────────────────────────────────────────
    #[test]
    fn test_give_and_steal_split_item_and_target() {
        assert_eq!(
            parse("give lantern bob"),
            Some(Command::Give {
                item: "lantern".into(),
                target: "bob".into()
            })
        );
        assert_eq!(
            parse("steal coin grimmok"),
            Some(Command::Steal {
                item: "coin".into(),
                target: "grimmok".into()
            })
        );
        // Either half missing is unknown.
        assert_eq!(parse("give"), None);
        assert_eq!(parse("give lantern"), None);
        assert_eq!(parse("steal"), None);
        assert_eq!(parse("steal coin"), None);
    }

    #[test]
    fn test_give_split_keeps_quoted_item_together() {
        assert_eq!(
            parse("give \"brass lantern\" bob"),
            Some(Command::Give {
                item: "\"brass lantern\"".into(),
                target: "bob".into()
            })
        );
        // Multi-word beings still work when the item is one word.
        assert_eq!(
            parse("give sword Grimmok Ironhand"),
            Some(Command::Give {
                item: "sword".into(),
                target: "Grimmok Ironhand".into()
            })
        );
        // Selector prefixes stay glued to a quoted phrase.
        assert_eq!(
            parse("give 2.\"brass lantern\" bob"),
            Some(Command::Give {
                item: "2.\"brass lantern\"".into(),
                target: "bob".into()
            })
        );
        // An unterminated quote is unknown, not a half-split guess.
        assert_eq!(parse("give \"brass bob"), None);
    }

    #[test]
    fn test_give_split_all_runs_to_last_token() {
        assert_eq!(
            parse("give all bob"),
            Some(Command::Give {
                item: "all".into(),
                target: "bob".into()
            })
        );
        assert_eq!(
            parse("give all sword bob"),
            Some(Command::Give {
                item: "all sword".into(),
                target: "bob".into()
            })
        );
        assert_eq!(
            parse("give all.sword bob"),
            Some(Command::Give {
                item: "all.sword".into(),
                target: "bob".into()
            })
        );
        assert_eq!(
            parse("give all \"brass lantern\" bob"),
            Some(Command::Give {
                item: "all \"brass lantern\"".into(),
                target: "bob".into()
            })
        );
        // A lone `all` has no being half.
        assert_eq!(parse("give all"), None);
        // `alloy` is a plain word, not an `all` item.
        assert_eq!(
            parse("give alloy bob"),
            Some(Command::Give {
                item: "alloy".into(),
                target: "bob".into()
            })
        );
    }

    #[test]
    fn test_give_split_multibyte_item_is_literal() {
        // Multibyte words must not panic the `all` boundary check, and they
        // split like any other first token.
        assert_eq!(
            parse("give épée bob"),
            Some(Command::Give {
                item: "épée".into(),
                target: "bob".into()
            })
        );
    }
    // ── Quit ──────────────────────────────────────────────────────
    #[test]
    fn test_give_and_steal_reject_text_glued_to_quote() {
        // Without a delimiter after the closing quote the item/being divide
        // is a guess, so both verbs are unknown — never a half-split move.
        assert_eq!(parse("give \"sword\"bob"), None);
        assert_eq!(parse("give \"brass lantern\"x bob"), None);
        assert_eq!(parse("steal \"coin\"bob"), None);
        assert_eq!(parse("steal 2.\"coin\"x bob"), None);
        // Whitespace-delimited quotes still parse on both verbs.
        assert_eq!(
            parse("steal \"brass lantern\" bob"),
            Some(Command::Steal {
                item: "\"brass lantern\"".into(),
                target: "bob".into()
            })
        );
    }

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

    // ── Reboot / copyover (admin; same countdown shape as shutdown) ──
    #[test]
    fn test_reboot_with_count() {
        assert_eq!(parse("reboot 10"), Some(Command::Reboot { seconds: 10 }));
    }

    #[test]
    fn test_reboot_defaults_to_30() {
        assert_eq!(parse("reboot"), Some(Command::Reboot { seconds: 30 }));
        assert_eq!(parse("reboot abc"), Some(Command::Reboot { seconds: 30 }));
    }

    #[test]
    fn test_copyover_with_count() {
        assert_eq!(
            parse("copyover 10"),
            Some(Command::Copyover { seconds: 10 })
        );
    }

    #[test]
    fn test_copyover_defaults_to_30() {
        assert_eq!(parse("copyover"), Some(Command::Copyover { seconds: 30 }));
        assert_eq!(
            parse("copyover abc"),
            Some(Command::Copyover { seconds: 30 })
        );
    }

    #[test]
    fn test_new_admin_verbs_do_not_steal_prefixes() {
        // `reboot`/`copyover` are deprioritized: short prefixes still reach
        // the older verbs; only long unambiguous prefixes reach the new ones.
        assert_eq!(
            parse("r hello"),
            Some(Command::Reply {
                text: "hello".into()
            })
        );
        assert_eq!(
            parse("re hello"),
            Some(Command::Reply {
                text: "hello".into()
            })
        );
        assert_eq!(parse("reb 10"), Some(Command::Reboot { seconds: 10 }));
        assert_eq!(parse("c"), Some(Command::Commands));
        assert_eq!(parse("co"), Some(Command::Commands));
        assert_eq!(parse("copy 10"), Some(Command::Copyover { seconds: 10 }));
    }

    #[test]
    fn test_open_close_parse_and_keep_prefixes() {
        // Bare `open`/`close` are unknown (like get/drop); directions parse
        // full or abbreviated. Deprioritized: `o` still reaches `ooc`, `c`
        // still reaches `commands`/`config`, only long prefixes reach these.
        assert_eq!(parse("open"), None);
        assert_eq!(parse("close"), None);
        assert_eq!(
            parse("open east"),
            Some(Command::Open {
                direction: Cardinal::East
            })
        );
        assert_eq!(
            parse("open e"),
            Some(Command::Open {
                direction: Cardinal::East
            })
        );
        assert_eq!(
            parse("close north"),
            Some(Command::Close {
                direction: Cardinal::North
            })
        );
        assert_eq!(parse("open nowhere"), None);
        assert_eq!(
            parse("ooc anyone here?"),
            Some(Command::Channel {
                channel: "ooc".to_string(),
                text: "anyone here?".to_string()
            })
        );
        assert_eq!(parse("c"), Some(Command::Commands));
        assert_eq!(
            parse("op east"),
            Some(Command::Open {
                direction: Cardinal::East
            })
        );
        assert_eq!(
            parse("cl north"),
            Some(Command::Close {
                direction: Cardinal::North
            })
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

    #[test]
    fn test_map_bare_and_abbreviation() {
        // The `m` prefix is uncontested: both the word and the abbreviation
        // reach `map`, while anything with arguments is unknown.
        assert_eq!(parse("map"), Some(Command::Map));
        assert_eq!(parse("m"), Some(Command::Map));
        assert_eq!(parse("ma"), Some(Command::Map));
        assert_eq!(parse("map foo"), None);
    }

    #[test]
    fn test_config_list_show_set() {
        assert_eq!(
            parse("config"),
            Some(Command::Config {
                key: None,
                value: None
            })
        );
        assert_eq!(
            parse("config minimap"),
            Some(Command::Config {
                key: Some("minimap".into()),
                value: None
            })
        );
        assert_eq!(
            parse("config minimap off"),
            Some(Command::Config {
                key: Some("minimap".into()),
                value: Some("off".into())
            })
        );
        // Values are single tokens: a third word is unknown.
        assert_eq!(parse("config minimap off now"), None);
    }

    #[test]
    fn test_config_keeps_older_c_prefixes() {
        // `config` registers before `commands`, so the shared `c`/`co`
        // abbreviations still reach the older verb. `con` is not a prefix of
        // `commands` at all (`com` vs `con`) — it unambiguously reaches
        // `config`, as does `conf`.
        assert_eq!(parse("c"), Some(Command::Commands));
        assert_eq!(parse("co"), Some(Command::Commands));
        assert_eq!(
            parse("con"),
            Some(Command::Config {
                key: None,
                value: None
            })
        );
        assert_eq!(
            parse("conf"),
            Some(Command::Config {
                key: None,
                value: None
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
            Some(Command::Channel {
                channel: "say".to_string(),
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
            Some(Command::Channel {
                channel: "say".to_string(),
                text: rest.to_string(),
            })
        });
        r.register("nordic", |rest| {
            Some(Command::Channel {
                channel: "yell".to_string(),
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
            Some(Command::Channel {
                channel: "yell".to_string(),
                text: "".to_string()
            })
        );
        // "not" matches only note
        assert_eq!(
            r.resolve("not", ""),
            Some(Command::Channel {
                channel: "say".to_string(),
                text: "".to_string()
            })
        );
    }
}
