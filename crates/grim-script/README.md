# `grim-script`
> Sandboxed Lua triggers: scripted mob reactions on room entry/exit.

**Role:** vertical — mob scripting / Lua triggers
**Depends on:** `grim-actor`, `grim-channel`, `grim-core`, `grim-text`

## Components
| Component | File | Purpose |
|---|---|---|
| `ScriptTriggers` | `src/trigger.rs` | Every script trigger on one creature, in blueprint order (`CompiledTrigger { on, bytecode }`). |
| `TriggerDef` | `src/trigger.rs` | Blueprint shape (`{on, script}` with inline Lua); compiled at spawn. |

## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|
| `watch_transitions` | `Update` | `src/watch.rs` | Reads `AttemptEnter`/`AttemptLeave`/`Enter`/`Leave`; fires matching triggers on scripted creatures in the affected room (mover excluded); routes speech + failure pages. |

## Commands
Player-facing verbs and where to find their handlers.
| Command | Handler | Summary |
|---|---|---|
| — | — | No player verbs. Mobs speak via `say(text)` inside scripts. |

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `TriggerKind` | serde enum (`enter`/`leave`/`attempt_enter`/`attempt_leave`) | `src/trigger.rs` |
| `ChannelMessage` | Message (output, from `grim-channel`) | `src/watch.rs` (mob speech on the `say` channel) |
| `InfoMessage` | Message (output, from `grim-core`) | `src/watch.rs` (failure pages to online admins) |

## Notes
- **Sandbox:** fresh Lua state per firing; stdlib is `math`/`string`/`table`/`utf8` only, and `load`/`os`/`io`/`require`/`print`/etc. are explicitly nilled (`STRIPPED_GLOBALS`, pinned by tests). 256 KiB memory cap + 100k-instruction budget turn runaways into errors.
- **Errors never disable:** a failing trigger logs + pages every online admin (`script.trigger.failed`) and fires again next transition.
- **Attempts are observe-only:** `Attempt*` carries no veto power yet; it lets scripts greet intent (`attempt_leave`) separately from arrival (`enter`).
- **One plugin:** `ScriptPlugin` (`src/plugin.rs`) only adds the watcher. Compose after `ActorPlugin` (emits the transition events) + `ChannelPlugin` (owns the `say` channel).
- Precompile-at-spawn: `compile()` validates syntax when the blueprint stamps, so typos break at startup, not on a player's move.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
