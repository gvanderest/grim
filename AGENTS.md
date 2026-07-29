# GRIM Engine — Agent Guide

AI agent instructions for this repo. For project docs (architecture, roadmap, conventions), see [README.md](./README.md) and [ARCHITECTURE.md](./docs/ARCHITECTURE.md).

## Architecture

**[docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) is authoritative for the target
architecture, and [CONTEXT.md](./CONTEXT.md) for vocabulary.** Read both before
designing anything. This file describes how to work in the repo and what the code
looks like *today*.

Current shape — 4 event-passing layers, no layer calls another's functions:

```
Protocol  ─→  Client  ─→  Engine  → Persistence
```

⚠️ "Client" is retired in the target architecture; it was carrying three unrelated
meanings. See CONTEXT.md.

## Crate Map (Current)

| Crate | Purpose |
|---|---|
| `grim-color` | Colour markup, ANSI rendering, palette, `escape_codes`. No Bevy, no serde |
| `grim-text` | Text catalog: `tr`/`tr!`, inlined defaults. Depends only on `grim-color`. No Bevy |
| `grim-command` | `CommandRegistry<C>` — generic, resource-ready. Exact-then-prefix resolution, `prioritize`/`deprioritize`, `contested_prefixes`. Bevy-only |
| `grim-engine-types` | Wire/game events, components, validation; re-exports `grim-color` |
| `grim` | Engine library: re-exports types + `grim_text::tr`, owns World/Social/Persistence plugins |
| `grim-client` | Session lifecycle, input parsing, output formatting |
| `grim-protocol-telnet` | TCP server, IAC negotiation, tokio↔Bevy bridge |
| `example-mud` | Binary (`crates/example-mud`): composes plugins, seeds world |

## Crate Map (Target)

See [ARCHITECTURE.md §4](./docs/ARCHITECTURE.md). Naming rules that matter when adding
a crate:

- **`grim-<system>`** for a subsystem, **`grim-<system>-<extension>`** only once a
  second real implementation exists.
- **There is no `grim-core-*`.** The `grim-` prefix already marks a crate as ours.
- Not every crate is a plugin. A crate with no `App` state (pure functions) stays a
  plain library — do not add an empty `impl Plugin`.

## Key Conventions for Agents

These describe **current** code. Several are slated to change — ARCHITECTURE.md §8
lists every gap. Do not treat them as targets: notably "last registered wins" command
resolution and the closed `Command` enum are documented as defects, not conventions to
extend.

- **`\n` everywhere**: Code uses `\n` for newlines. The protocol layer (`send_network_commands`) converts `\n` → `\r\n` before writing to TCP.
- **Text catalog**: Use `grim::tr!("key", var = value)` (or `grim::tr(key, args)`) for all author-facing strings — this is `grim-text`. It converts `{X` colour codes to `@xRGB`, then substitutes `%{var}` placeholders, escaping each value so it cannot inject colour. Defaults are inlined in `grim-text`. `rust-i18n`/`t!` and `locales/en.json` are gone. Colour rendering to ANSI (`ansi`, `convert_16color`, `escape_codes`) is `grim-color`, re-exported at `grim::color::*`.
- **User input filter**: Protocol layer strips all non-printable ASCII (32-126) from user input before creating `ClientInput`, preventing ANSI/control code injection.
- **`Exit` vs `Exits`**: The component is `Exits { exits: HashMap<Cardinal, Entity> }`. On a Room entity.
- **`Name` vs `GrimName`**: Import aliased from `grim` as `GrimName` in binary/client to avoid collisions with Bevy's `Name`.
- **Single-letter directions** (`n`/`e`/`s`/`w`/`u`/`d`) work via prefix matching against the `CommandRegistry`. Directions are registered last in `parser.rs::build_registry()`, and `register` puts each new command at the front of the priority ordering, so directions win single-character input.
- **Command resolution**: `grim::CommandRegistry<Command>` (from `grim-command`). Commands are registered by name + `fn(&str) -> Option<Command>` factory. Resolution is case-insensitive: exact name first, then highest-priority prefix. Priority is explicit and reorderable via `prioritize`/`deprioritize` — not `max(entry_idx)`. `l` is a registered name; `n` matches `north` via prefix. `init_registry` logs any contested prefix at startup. Still held in a `OnceLock` (not yet a live resource) — see ARCHITECTURE.md §8.
- **Character takeover**: If a character is already online (has `Player` with `connection: Some(...)`) and another session selects it, the old session receives "Someone else has logged into this character." and is immediately disconnected. The new session proceeds normally.
- **Online indicator**: The character select menu shows "(online)" for characters with a live `Player` connection and "(linkdead)" for linkdead characters.
- **Multiple characters per account**: Allowed simultaneously. No check prevents multiple characters from the same account being `InGame` at the same time.
- **save_on_disconnect guard**: On connection close, the character is only marked linkdead if it doesn't already have a live `Player` connection (guards against stale `ConnectionClosed` events from a takeover).
- **Ownership/visibility checks must fail closed.** When a lookup that *gates* what a session may see can itself fail — most commonly a `commands.spawn` entity queried in the same tick before it is flushed — a failed lookup must show **nothing**, never fall through to unfiltered data. This bit `show_character_menu`: the account-ownership filter sat inside `if let Ok(account) = accounts.get(..)`, so a just-created account (unflushed entity) skipped the filter and listed every character in the world. Resolve the gating entity once, up front; if it is unavailable, return the empty/denied result. Mirror the pattern already used on the selection path (`let Ok(..) = accounts.get(..) else { continue };`).

## Architecture Decisions

When developing new features, start by asking:

1. **Does this already fit into the architecture?** → Implement as a plugin
2. **Does this require a change to the architecture?** → Document in `docs/ARCHITECTURE.md`
3. **Does this add new functionality needing API-level discussion?** → Open an issue

**Reference:** `docs/ARCHITECTURE.md` for current architecture decisions.

## Workflow

1. **Branch pre-check** — check current branch; if wrong, branch from `main`
2. **Branch** from `main`
3. **Commit** incrementally
4. **Push** the branch
5. **Create a PR** — `gh pr create --fill --base main`. Pushing without a PR is unfinished.
6. **CI** checks: build, lint, test
7. **Human review**
8. **Squash merge** into `main`

## Editing Discipline

1. **Re-ground after every edit.** Fresh snapshot tag + renumber from edit response or fresh `read`.
2. **Verify structure before chaining.** `read` the affected area before the next edit.
3. **Prefer rewrite over surgical patch when code is young.** Files <5 edits old → `write` from scratch.
4. **Read the whole function.** Elided ranges = unseen — expand range.
5. **Test immediately after the last edit.**
6. **Explicit character sets, not ASCII ranges.** `'k'..='w'` ≠ `krgybmcw`. Use `'k' | 'r' | 'g' | 'y' | 'b' | 'm' | 'c' | 'w'`.
7. **Update README.md** when architecture, roadmap, or conventions change — not this file.
8. **NEVER use `--no-verify` on commits.** Pre-commit hooks (lint, fmt, coverage) are mandatory. If they block, fix what they catch.
9. **Codify every root cause.** When the user asks why something broke, when there's confusion or frustration, or when an unstated assumption surfaces — add a rule to AGENTS.md. These signals mean something wasn't obvious. Write it down so the next agent doesn't repeat it.
10. **100% coverage target, 90% floor.** All code should be tested. The goal is 100% line coverage. The 90% threshold (in the Makefile) exists only as a safety net for genuinely uncoverable lines (unreachable defensive branches, language limitations). Any gap below 90% is a bug — fix it. If a file can't reach 90%, ratchet down in 5% decrements, re-evaluating at each step.