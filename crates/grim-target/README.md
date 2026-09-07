# `grim-target`
> Parse a textual `<target>` into a spec and resolve it against candidates.

**Role:** horizontal (primitives) — target parsing + keyword query
**Depends on:** `bevy` (only, for `Entity` ordering)

Every verb that names something (`look`, `get`/`drop`, `give`/`steal`, `tell`)
shares this crate: `parse` turns raw text into a [`TargetSpec`](src/parse.rs)
(terms + [`Selector`](src/parse.rs)), `rank` scores one name (+ keywords)
against the terms, and `query` ranks a candidate iterator best-first and
applies the selector.

## Components
| Component | File | Purpose |
|---|---|---|

## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|

## Commands
Player-facing verbs and where to find their handlers.
| Command | Handler | Summary |
|---|---|---|
| `2.sword` | `parse_target` (`src/parse.rs`) | Offset: the 2nd best match (being targets: `look 2.goblin`). |
| `3*sword` | `parse_target` (`src/parse.rs`) | Quantity: up to 3 best matches (item verbs only). |
| `all [words]` / `all.words` | `parse_target` (`src/parse.rs`) | Every match, best first (item verbs only; bare `all` needs no terms). |
| `"quoted phrase"` | `parse_target` (`src/parse.rs`) | Multi-term AND: every word must prefix-match (`2."pot health"` = 2nd health potion). |

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `TargetSpec` | Type (parsed target) | `src/parse.rs` |
| `ParseOptions` | Type (`ITEM` / `BEING` presets) | `src/parse.rs` |
| `Rank` | Type (tier, name length, name) | `src/rank.rs` |

## Notes
- A plain library — no plugin, no game types. `grim-actor`, `grim-object`, and
  `grim-channel` all depend on it; it depends on Bevy alone (like `grim-command`).
- One term keeps the legacy `look` ranking exactly (exact name, exact keyword,
  shortest-prefix name); groups AND at the prefix tier, matching name words and
  keywords. Ties fall to the lowest entity id.
- Quantity and offset never combine (`10*2.sword` is invalid); a disallowed
  prefix stays literal (`look all` searches for something named "all").
- Quotes are stripped before term-splitting — they matter one layer up, where
  `give`/`steal` split `<item> <target>` (`grim-scene`'s `split_transfer` keeps
  a quoted phrase on one side of that divide).

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
