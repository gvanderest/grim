# GRIM Engine — Agent Guide

AI agent instructions for this repo. For project docs (architecture, roadmap, conventions), see [README.md](./README.md).

## Architecture

4 event-passing layers, no layer calls another's functions:

```
Protocol  ─→  Client  ─→  Engine  → Persistence
```

## Crate Map

| Crate | Purpose |
|---|---|
| `grim` | Engine library: components, events, cardinals, validation, plugins |
| `grim-client` | Session lifecycle, input parsing, output formatting |
| `grim-protocol-telnet` | TCP server, IAC negotiation, tokio↔Bevy bridge |
| `mud-example` (root) | Binary: composes plugins, seeds world |

## Key Conventions for Agents

- **`\n` everywhere**: Code uses `\n` for newlines. The protocol layer (`send_network_commands`) converts `\n` → `\r\n` before writing to TCP.
- **Color markup + i18n**: Use `grim::color::tr()` for locale strings containing color codes. The `tr()` function reads `locales/en.json`, converts `{X}` 16-color codes to `@xRGB` format (so i18n's `{var}` interpolation doesn't eat them), then replaces `%{var}` placeholders. Plain strings without colors still use `t!()`.
- **User input filter**: Protocol layer strips all non-printable ASCII (32-126) from user input before creating `ClientInput`, preventing ANSI/control code injection.
- **`Exit` vs `Exits`**: The component is `Exits { exits: HashMap<Cardinal, Entity> }`. On a Room entity.
- **`Name` vs `GrimName`**: Import aliased from `grim` as `GrimName` in binary/client to avoid collisions with Bevy's `Name`.
- **Single-letter directions** (`n`/`e`/`s`/`w`/`u`/`d`) work via prefix matching against the `CommandRegistry` trie. Directions are registered last in `parse.rs::build_registry()` so they always win for single-character input.
- **Command resolution**: Uses a `CommandRegistry` (trie-based, `grim::command_registry`). Commands are registered by name + factory function. Resolution is case-insensitive prefix matching with "last registered wins" for ties. No separate "shortcut" aliases are needed — `l` is a registered name, `n` matches `north` via prefix.
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