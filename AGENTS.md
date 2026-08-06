# GRIM Engine — Agent Guide

AI agent instructions for this repo. For project docs, see [README.md](./README.md) (what GRIM is + the crate map with per-crate README links), [ARCHITECTURE.md](./docs/ARCHITECTURE.md) (target architecture + roadmap), and [CONTEXT.md](./CONTEXT.md) (glossary).

## Architecture

**[docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) is authoritative for the target
architecture, and [CONTEXT.md](./CONTEXT.md) for vocabulary.** Read both before
designing anything. This file describes how to work in the repo and what the code
looks like *today*.

Current shape — event-passing subsystems behind a facade. The dependency
direction now matches the target: `grim` depends on the subsystems and re-exports
them (`GrimDefaultPlugins`); nothing depends back on the facade.

⚠️ "Client" is retired in the vocabulary; the session crate is `grim-scene`. But
it still uses the `ClientState` enum internally — the scene-stack entity model is
deferred (ARCHITECTURE.md §8). See CONTEXT.md.

## Crate Map (Current)

| Crate | Purpose |
|---|---|
| `grim-color` | Colour markup, ANSI rendering, palette, `escape_codes`. No Bevy, no serde |
| `grim-text` | Text catalog: `tr`/`tr!`, inlined defaults. Depends only on `grim-color`. No Bevy |
| `grim-command` | `CommandRegistry<C>` — generic, resource-ready. Exact-then-prefix resolution, `prioritize`/`deprioritize`, `contested_prefixes`. Bevy-only |
| `grim-networking` | `Connection` component + wire events (`ConnectionInput`/`Output`, `ConnectionEstablished`/`Closed`, `DisconnectRequest`). Bevy-only |
| `grim-networking-telnet` | `TelnetPlugin`: TCP server, IAC negotiation, tokio↔Bevy bridge, ANSI render |
| `grim-core` | Game events, components, validation; re-exports `grim-color` |
| `grim-scene` | `ScenePlugin`: session lifecycle (`ClientState`), input parsing, output formatting. Owns the `CommandRegistry` resource |
| `grim-world` | Being-free world topology (rooms/areas/exits + room-address lookups + `RoomLocation`); `WorldPlugin` (world-event vocabulary) + `ShutdownPlugin` (SIGTERM signal + countdown machinery). Also owns race/class registries |
| `grim-actor` | The "beings": `Actor` base (race/level/gender, on every being) + PC-only `Character` (account/roles/class/title/restrings/last_room) + `Creature` mob marker + `Player`/`InRoom`/`Linkdead`/`OutputHistory`/`Role`, plus the `StoredCharacter` flat disk DTO. Names live in the `Name` component. Being-reading verbs (`look`/`move`/`goto`/`quit`/`title` + admin `shutdown` gate). `ActorPlugin`. Depends on `grim-world`, never the reverse |
| `grim-channel` | `ChannelPlugin`: say/yell/ooc handlers |
| `grim-persistence` | `PersistencePlugin`: account/character load + save-on-disconnect |
| `grim` | Facade: depends on and re-exports every subsystem; `GrimDefaultPlugins` group. No code of its own |
| `example-mud` | Binary (`crates/example-mud`): `GrimDefaultPlugins` + world seed |

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
- **Command resolution**: `grim::CommandRegistry<Command>` (from `grim-command`). Commands are registered by name + `fn(&str) -> Option<Command>` factory. Resolution is case-insensitive: exact name first, then highest-priority prefix. Priority is explicit and reorderable via `prioritize`/`deprioritize` — not `max(entry_idx)`. `l` is a registered name; `n` matches `north` via prefix. It is a live Bevy **resource** (`grim-scene` inserts `parser::command_registry()`, which logs contested prefixes), threaded into `handle_client_input` via the `SessionRes` `SystemParam`. The `OnceLock` is gone.
- **Player presence = online.** `Player` is attached only while a connection is driving the character (`Player { connection: Entity }`, non-optional). **Online ⇔ the character has a `Player`; linkdead ⇔ it has a `Character` and no `Player`** (marked `Linkdead`). There is no "linkdead `Player`".
- **Character takeover**: If a character is already online (has a `Player`) and another session selects it, the old session's connection receives "Someone else has logged into this character." and is disconnected; the new session inserts its own `Player` (with the new connection) and proceeds. The old connection's later `ConnectionClosed` must NOT re-mark the character linkdead — see the guard below.
- **Online indicator**: The character select menu shows "(online)" for characters that have a `Player` and "(linkdead)" for linkdead characters (`Linkdead`, no `Player`).
- **Multiple characters per account**: Allowed simultaneously. No check prevents multiple characters from the same account being `InGame` at the same time.
- **save_on_disconnect guard**: On connection close, `save_on_disconnect` **removes** the `Player` and **inserts** `Linkdead` (the disk save routes through `StoredCharacter`; `OutputHistory` transfers to the character entity). It skips this only if the character was taken over — its live `Player.connection` differs from the closing connection — so a stale `ConnectionClosed` from a takeover never disturbs the new session's live `Player`.
- **Ownership/visibility checks must fail closed.** When a lookup that *gates* what a session may see can itself fail — most commonly a `commands.spawn` entity queried in the same tick before it is flushed — a failed lookup must show **nothing**, never fall through to unfiltered data. This bit `show_character_menu`: the account-ownership filter sat inside `if let Ok(account) = accounts.get(..)`, so a just-created account (unflushed entity) skipped the filter and listed every character in the world. Resolve the gating entity once, up front; if it is unavailable, return the empty/denied result. Mirror the pattern already used on the selection path (`let Ok(..) = accounts.get(..) else { continue };`).

## Architecture Decisions

**Prefer existing crates over rolling your own.** Before writing non-trivial
infrastructure (protocol handling, OS/syscall glue, serialization, async
primitives, anything with a well-known name), research crates.io first: is there
a mature, maintained crate that does this? Check download count and last-updated
date. Default to gluing a proven crate over hand-rolling — hand-rolled infra is a
maintenance and correctness liability. Only reimplement when no crate fits, the
fit is poor, or the dependency cost is clearly not worth it — and say why. Do this
research at the *start* of a feature, before designing the bespoke version.

When developing new features, start by asking:

1. **Is there a crate for this?** → Research crates.io before building infra (see above)
2. **Does this already fit into the architecture?** → Implement as a plugin
3. **Does this require a change to the architecture?** → Document in `docs/ARCHITECTURE.md`
4. **Does this add new functionality needing API-level discussion?** → Open an issue

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
7a. **Every crate has a `README.md` following [`docs/README.template.md`](./docs/README.template.md).** It's the map to that crate — Components, Systems, and (GRIM-specifically) **Commands with the file each handler lives in**, plus Resources/Events. Keep it current *in the same change*: whenever you add, move, or remove a Component / System / Resource / Event / Command, update that crate's README so the pointers stay true — the Commands→handler table especially (a stale handler pointer is worse than none). Findability is the point; treat the README as part of the code, not an afterthought.
8. **NEVER use `--no-verify` on commits.** Pre-commit hooks (lint, fmt, coverage) are mandatory. If they block, fix what they catch.
9. **Codify every root cause.** When the user asks why something broke, when there's confusion or frustration, or when an unstated assumption surfaces — add a rule to AGENTS.md. These signals mean something wasn't obvious. Write it down so the next agent doesn't repeat it.
10. **100% coverage target, 90% floor.** All code should be tested. The goal is 100% line coverage. The 90% threshold (in the Makefile) exists only as a safety net for genuinely uncoverable lines (unreachable defensive branches, language limitations). Any gap below 90% is a bug — fix it. If a file can't reach 90%, ratchet down in 5% decrements, re-evaluating at each step.