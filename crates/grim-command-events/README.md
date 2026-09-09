# `grim-command-events`
> Semantic intent events emitted by command parsing: one `Message` type per player intent.

**Role:** horizontal (primitives) — command intent vocabulary
**Depends on:** `grim-core` (`Cardinal`), `bevy` (`Message` derive only)

Parsers resolve input and emit these intents; subsystem observers consume them.
This crate holds events only — no registry, no handlers, no plugin.

## Components
| Component | File | Purpose |
|---|---|---|
| None | — | Event types only; no components. |

## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|
| None | — | — | No systems; intents are consumed by subsystem observers. |

## Commands
Player-facing verbs and where to find their handlers.
| Command | Handler | Summary |
|---|---|---|
| None | — | Intents, not commands: the registry (`grim-command`) and handlers live elsewhere. |

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `MoveIntent` | Message | `src/movement.rs` |
| `LookIntent` | Message | `src/movement.rs` |
| `SayIntent` | Message | `src/social.rs` |
| `YellIntent` | Message | `src/social.rs` |
| `OocIntent` | Message | `src/social.rs` |
| `TellIntent` | Message | `src/social.rs` |
| `ReplyIntent` | Message | `src/social.rs` |
| `ShutdownIntent` | Message | `src/admin.rs` |
| `GotoIntent` | Message | `src/admin.rs` |
| `GechoIntent` | Message | `src/admin.rs` |
| `WhoIntent` | Message | `src/info.rs` |
| `WhereIntent` | Message | `src/info.rs` |
| `CommandsIntent` | Message | `src/info.rs` |
| `AreasIntent` | Message | `src/info.rs` |
| `QuitIntent` | Message | `src/quit.rs` |

## Notes
- A plain library — no `Plugin` impl (ARCHITECTURE.md §2: no `App` state, no plugin).
- Every intent derives `Message + Debug` and carries the acting `actor: Entity`.
- Re-exported at the facade root (`grim::MoveIntent`, …).
- A halfway step toward ADR-0004's per-command typed dispatch: one shared intent
  vocabulary today, per-plugin event types when that ADR lands.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
