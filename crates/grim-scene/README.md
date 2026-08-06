# `grim-scene`
> The session subsystem: lifecycle (`ClientState`), input parsing/routing, output formatting + per-recipient broadcast, admin-gated dispatch, and copyover resume.

**Role:** horizontal (infrastructure) — session / networking↔command bridge
**Depends on:** `grim-engine-types`, `grim-networking`, `grim-command`, `grim-text`, `grim-color`, `grim-persistence`, `grim-world`, `grim-actor`

## Components
| Component | File | Purpose |
|---|---|---|
| `ConnectedAt(DateTime<Utc>)` | `src/session.rs` | When this session connected (used by the WHO list ordering). |
| `Client` | (`grim-engine-types::components`) | Per-connection session state; consumed here, defined upstream. |
| `ClientState` | (`grim-engine-types::components`) | Login/creation/in-game state machine; drives input routing. |

## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|
| `validate_registries` | `Startup` | `src/plugin.rs` | Panics on a mis-seeded world (empty race registry / no tier-1 class). |
| `handle_connection_established` | `Update` | `src/input.rs` | Spawns a `Client` and greets a new connection. |
| `handle_connection_resumed` | `Update` | `src/resume.rs` | Re-attaches a session after copyover / reconnect. |
| `handle_client_input` | `Update` | `src/input.rs` | Routes a line of input by `ClientState` (login, creation, in-game …). |
| `process_command_queue` | `Update` | `src/command.rs` | Drains queued `Command`s under cooldown; emits `EngineCommand`; handles `quit` (save+despawn). |
| `format_output` | `Update` | `src/output.rs` | Renders domain events per-recipient into `ConnectionOutput`. |
| `format_server_broadcast` | `Update` | `src/output.rs` | Renders `ServerBroadcast` (e.g. shutdown warnings) to all sessions. |
| `capture_output` | `Update` | `src/output.rs` | Collects output for flushing to connections. |

## Commands
Parsed by `grim-scene`'s registry (`src/parser.rs`); these verbs are handled **session-locally** in `src/command.rs` (they never reach the engine queue).
| Command | Handler | Summary |
|---|---|---|
| `who` | `handle_ingame` → `format_who` (`src/command.rs`) | List online characters (admins first, then level/connect/name). |
| `where` | `handle_ingame` → `format_where` (`src/command.rs`) | Show where players are located. |
| `areas` | `handle_ingame` → `format_areas` (`src/command.rs`) | List known areas. |
| `commands` | `handle_ingame` → `format_commands` (`src/formatter.rs`) | Show the command list. |
| `help` | `handle_ingame` → `format_commands` (`src/command.rs`) | Alias for `commands` (parser maps `help` → `Command::Commands`). |

Other verbs (`look`, `move`, `say`, `shutdown`, …) are parsed here then routed: most enqueue via `process_command_queue`; admin-gated ones (`shutdown`/`goto`/`gecho`) go through `dispatch_admin_gated` (masked as unknown for non-admins).

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `ReservedNamePrefixes` | Resource | `src/validation.rs` |
| `CommandRegistry<Command>` | Resource (built by `command_registry()`) | `src/parser.rs` |
| `EngineCommand` | Message (emitted to engine) | `src/command.rs` |
| `ConnectionOutput` | Message (emitted; from `grim-networking`) | `src/output.rs` |
| `InfoMessage` | Message (consumed → rendered) | `src/output.rs` |
| `LookRoom` / `LookEntity` / `MoveEvent` | Message (consumed → rendered) | `src/output.rs` |
| `SayEvent` / `YellEvent` / `OocEvent` / `GlobalEcho` | Message (consumed → rendered) | `src/output.rs` |
| `LoginAnnounce` / `LogoutAnnounce` / `LinkdeadAnnounce` | Message (session announces) | `src/output.rs`, `src/command.rs` |
| `ServerBroadcast` | Message (consumed → rendered) | `src/output.rs` |

## Notes
- Single plugin: `ScenePlugin`. It is the **bridge** between `grim-networking` and `grim-command` (ARCHITECTURE.md §5.2/§5.3): reads raw input, parses to `Command`, dispatches, and renders every domain event back per-recipient.
- Input routing is a flat `match` on `ClientState` in `handle_client_input`, one arm per state delegating to `login`/`character`/`creation`/`command` modules.
- Parser (`src/parser.rs`) owns direction aliases (`n`/`north` …), the `whisper`→`tell` alias, `help`→`commands`, and prefix resolution (last-registered wins on ties; e.g. `n` → north over social).
- `!` repeats the last input; blank lines re-trigger the prompt.
- Admin-gated commands are byte-identical to "unknown command" for non-admins (no information leak) — see `dispatch_admin_gated`.
- Scene stack (§5.3) still uses the `ClientState` enum; the typed scene-stack redesign is deferred (see MEMORY / ARCHITECTURE §5.3).

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
