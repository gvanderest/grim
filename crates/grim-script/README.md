# `grim-script`
> Sandboxed Lua triggers: scripted mob reactions on room entry/exit.

**Role:** vertical — mob scripting / Lua triggers
**Depends on:** `grim-actor`, `grim-channel`, `grim-core`, `grim-text`

## Components
| Component | File | Purpose |
|---|---|---|
| `ScriptTriggers` | `src/trigger.rs` | Every script trigger on one creature, in blueprint order (`CompiledTrigger { on, bytecode }`). |
## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|
| `on_attempt_leave` / `on_attempt_enter` | Observer (sync, in the movement pipeline) | `src/watch.rs` | Run departure/arrival-room scripts before placement; a `deny()` latches onto the attempt. |
| `on_leave` / `on_enter` | Observer (post-placement) | `src/watch.rs` | Run departure/arrival-room scripts on the committed facts. |

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
## Notes
- **Sandbox:** fresh Lua state per firing; stdlib is `math`/`string`/`table`/`utf8` only, and `load`/`os`/`io`/`require`/`print`/etc. are explicitly nilled (`STRIPPED_GLOBALS`, pinned by tests). 256 KiB memory cap + 100k-instruction budget turn runaways into errors.
- **Facts fire a tick after placement:** `Leave`/`Enter` queue into the movement `PendingFacts` buffer, so their greetings land in a later flush than the arrival they react to. Attempt-time speech is immediate.
- **Errors never disable:** a failing trigger logs + pages every online admin (`script.trigger.failed`) and fires again next transition.
- **Attempts are blockable:** a script calls `deny()` to latch a denial (after `say`ing its refusal — echo belongs to the denier); a denied move never places. Facts only observe.
- **One plugin:** `ScriptPlugin` (`src/plugin.rs`) adds the four observers plus the `PendingSpeech` flush. Compose after `ActorPlugin` (fires the transition events) + `ChannelPlugin` (owns the `say` channel).
- Precompile-at-spawn: `compile()` validates syntax when the blueprint stamps, so typos break at startup, not on a player's move.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
