# `grim-channel`
> Player-speech channels: `say`, `yell`, `ooc`, `tell`/`whisper`, `reply`, and admin `gecho`.

**Role:** vertical — player speech / communication channels
**Depends on:** `grim-core`, `grim-world`, `grim-actor`, `grim-text`

## Components
| Component | File | Purpose |
|---|---|---|
| `LastWhisperFrom(Entity)` | `src/whisper.rs` | Records the last player who whispered this entity, so `reply` can target them. |

## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|
| `handle_say` | `Update` | `src/commands/say.rs` | Reads `Command::Say`, emits `SayEvent` for the room plus a first-party `InfoMessage` echo. |
| `handle_yell` | `Update` | `src/commands/yell.rs` | Reads `Command::Yell`, emits `YellEvent` scoped to the actor's `Area`. |
| `handle_ooc` | `Update` | `src/commands/ooc.rs` | Reads `Command::Ooc`, emits `OocEvent` (out-of-character global). |
| `handle_gecho` | `Update` | `src/commands/gecho.rs` | Reads `Command::Gecho`, emits `GlobalEcho`. Re-checks admin (defense in depth). |
| `handle_tell` | `Update` | `src/commands/tell.rs` | Reads `Command::Tell`, fuzzy-matches the target player, delivers a private whisper. |
| `handle_reply` | `Update` | `src/commands/reply.rs` | Reads `Command::Reply`, whispers the entity's `LastWhisperFrom`. |

## Commands
Player-facing verbs and where to find their handlers.
| Command | Handler | Summary |
|---|---|---|
| `say <text>` | `handle_say` (`src/commands/say.rs`) | Broadcast to the current room; echoes "You say, '…'" to the speaker. |
| `yell <text>` | `handle_yell` (`src/commands/yell.rs`) | Broadcast to every room in the actor's area. |
| `ooc <text>` | `handle_ooc` (`src/commands/ooc.rs`) | Out-of-character global chat. |
| `tell <target> <text>` | `handle_tell` (`src/commands/tell.rs`) | Private message to one player (case-insensitive name prefix; `self` targets sender). |
| `whisper <target> <text>` | `handle_tell` (`src/commands/tell.rs`) | Alias for `tell` (parsed in `grim-scene`). |
| `reply <text>` | `handle_reply` (`src/commands/reply.rs`) | Whisper the last player who whispered you (`LastWhisperFrom`). |
| `gecho <text>` | `handle_gecho` (`src/commands/gecho.rs`) | Admin-only world-wide echo. Gated at dispatch in `grim-scene`; re-checked here. |

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `SayEvent` | Message (consumed; defined in `grim-core`) | `src/commands/say.rs` |
| `YellEvent` | Message (consumed) | `src/commands/yell.rs` |
| `OocEvent` | Message (consumed) | `src/commands/ooc.rs` |
| `GlobalEcho` | Message (consumed) | `src/commands/gecho.rs` |
| `EngineCommand` | Message (input) | each `src/commands/*.rs` |
| `InfoMessage` | Message (output echo) | each `src/commands/*.rs` |

## Notes
- One file per command under `src/commands/` (mirrors `grim-actor`'s convention); `src/plugin.rs` wires each command's `register(app)` into `ChannelPlugin`, and `src/whisper.rs` holds the `LastWhisperFrom` component plus the `deliver_whisper` helper shared by `tell`/`reply`.
- Command *parsing* (including the `whisper` alias) lives in `grim-scene`'s parser; this crate only handles resolved `Command` variants delivered via `EngineCommand`.
- Rendering / per-recipient attribution is the renderer's job (`grim-scene`'s `format_output`), not this crate's — handlers emit intent events only.
- The data-driven `add_channel(Channel { scope, eligibility, .. })` model with one shared `ChannelMessage` (ARCHITECTURE.md §7) is deferred with the typed-event dispatch (§5.2); today's distinct `Say/Yell/Ooc` events are the interim shape.
- `gecho` admin gating is enforced at dispatch (`grim-scene`) and re-verified in `handle_gecho` — fails closed (silent) for non-admins.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
