# `grim-scene`
> The session subsystem: in-game input parsing/routing, output formatting + per-recipient broadcast, admin-gated dispatch, and copyover resume.

**Role:** horizontal (infrastructure) — session / networking↔command bridge
**Depends on:** `grim-core`, `grim-networking`, `grim-command`, `grim-text`, `grim-color`, `grim-persistence`, `grim-world`, `grim-actor`

The pre-game phase (login / account-creation / character-select / MOTD) lives in
`grim-auth`, which layers on this crate (auth → scene). `grim-scene` routes only
lines whose session stack tops at `InGameScene`; stackless sessions are handled
by `grim-auth`.

## Components

| Component | File | Purpose |
|---|---|---|
| `ConnectedAt(DateTime<Utc>)` | `src/session.rs` | When this session connected (used by the WHO list ordering). |
| `SceneStack(Vec<Entity>)` | `src/scene_stack.rs` | Ordered scene stack on the session; input is interpreted by the top. Thin slice: only the in-game scene exists — pushed once at world entry (auth transition guard, copyover resume), read by the router, cascaded on despawn. |
| `InGameScene` | `src/scene_stack.rs` | Marker on the scene entity topping an in-world session. Missing/empty stack reads as not-in-game (fail closed). |
| `Client` | (`grim-core::components`) | Per-connection session state; consumed here, defined upstream. |
| `ClientState` | (`grim-core::components`) | Login/creation/in-game state machine; the in-game arm drives this crate, the pre-game arms drive `grim-auth`. Still the pre-game driver — the stack mirrors only world entry until ADR-0003 lands fully. |

## Systems

| System | Schedule | File | Purpose |
| `handle_ingame_input` | `Update` (`SceneSystems::InGameInput`) | `src/input.rs` | Routes a line whose session stack tops at `InGameScene` into an in-game command; skips stackless sessions (auth's job) and the line that just entered the world (`JustEnteredWorld`). |
| `handle_connection_resumed` | `Update` | `src/resume.rs` | Re-attaches a session after copyover / reconnect (skips login). |
| `process_command_queue` | `Update` | `src/command.rs` | Drains queued `Command`s under cooldown; emits `EngineCommand`; handles `quit` (save+despawn). |
| `format_output` | `Update` | `src/output.rs` | Renders domain events per-recipient into `ConnectionOutput`. |
| `format_server_broadcast` | `Update` | `src/output.rs` | Renders `ServerBroadcast` (e.g. shutdown warnings) to all sessions. |
| `capture_output` | `Update` | `src/output.rs` | Collects output for flushing to connections. |

## Commands
Parsed by `grim-scene`'s registry (`src/parser.rs`); these verbs are handled **session-locally** in `src/command.rs` (they never reach the engine queue).

| Command | Handler | Summary |
|---|---|---|
| `who` | `handle_ingame` → `format_who` (`src/command.rs`) | List online characters (admins first, then level/connect/name). |
| `finger <name>` | `handle_ingame` → `finger::format` (`src/finger.rs`) | Character sheet (name/level/gender/race/class + description), online or off-disk. |
| `desc …` | parser → engine queue (`src/parser.rs`, `grim-actor/src/commands/desc.rs`) | View/edit your description paragraphs (`clear`, `+ <line>`, `-` drops last). |
| `sockets` | `handle_ingame` → `format_sockets` (`src/sockets.rs`) | List live connections by id (admin-only; masked as unknown for others). |
| `where` | `handle_ingame` → `format_where` (`src/command.rs`) | Show where players are located. |
| `areas` | `handle_ingame` → `format_areas` (`src/command.rs`) | List known areas. |
| `commands` | `handle_ingame` → `format_commands` (`src/formatter.rs`) | Show the command list. |
| `help` | `handle_ingame` → `format_commands` (`src/command.rs`) | Alias for `commands` (parser maps `help` → `Command::Commands`). |

Other verbs (`look`, `move`, `say`, `shutdown`, …) are parsed here then routed: most enqueue via `process_command_queue`; engine-queued admin verbs (`shutdown`/`goto`/`gecho`) go through `dispatch_admin_gated` (masked as unknown for non-admins). `sockets` is also admin-gated + masked, but answered session-locally from a per-tick `ClientSnapshot` (a second `Client` query would conflict with the dispatcher's `&mut` borrow).

## Resources & Events

| Name | Kind (Resource/Message) | File |
|---|---|---|
| `JustEnteredWorld` | Resource (routing-split guard; pub) | `src/session.rs` |
| `SceneSystems` | `SystemSet` (pub; orders the pre-game system before in-game input) | `src/plugin.rs` |
| `CommandRegistry<Command>` | Resource (built by `command_registry()`) | `src/parser.rs` |
| `EngineCommand` | Message (emitted to engine) | `src/command.rs` |
| `ConnectionOutput` | Message (emitted; from `grim-networking`) | `src/output.rs` |
| `InfoMessage` | Message (consumed → rendered) | `src/output.rs` |
| `LookRoom` / `LookEntity` / `MoveEvent` | Message (consumed → rendered) | `src/output.rs` |
| `SayEvent` / `YellEvent` / `OocEvent` / `GlobalEcho` | Message (consumed → rendered) | `src/output.rs` |
| `LoginAnnounce` / `LogoutAnnounce` / `LinkdeadAnnounce` | Message (session announces) | `src/output.rs`, `src/command.rs` |
| `ServerBroadcast` | Message (consumed → rendered) | `src/output.rs` |

The shared render helpers in `src/formatter.rs` (`format_motd`, `format_selection_menu`, `parse_menu_choice`, `MenuItem`) are `pub` because the `grim-auth` pre-game flow reads them; the module is re-exported at `grim_scene::formatter`.

## Notes
- Single plugin: `ScenePlugin`. It is the **bridge** between `grim-networking` and `grim-command` (ARCHITECTURE.md §5.2/§5.3): reads raw in-game input, parses to `Command`, dispatches, and renders every domain event back per-recipient.
- **Input routing split (Phase 2b + scene-stack thin slice).** One `handle_client_input` became two systems: `handle_ingame_input` here (stack top is `InGameScene`) and `handle_pregame_input` in `grim-auth` (every pre-game `ClientState`). The pre-game system runs first (`.before(SceneSystems::InGameInput)`); a line that advances a session into the world also pushes its in-game scene there and records the connection in `JustEnteredWorld`, so this crate's system skips that line the same tick and routes by stack from the next tick on.
- Parser (`src/parser.rs`) owns direction aliases (`n`/`north` …), the `whisper`→`tell` alias, `help`→`commands`, and prefix resolution (last-registered wins on ties; e.g. `n` → north over social).
- `!` repeats the last input; blank lines re-trigger the prompt.
- Admin-gated commands are byte-identical to "unknown command" for non-admins (no information leak) — see `dispatch_admin_gated`.
- Scene stack (§5.3): thin slice landed — `SceneStack` + `InGameScene`, pushed at world entry, routing by top. Pre-game scenes, pop, and output policy are still deferred (ADR-0003).

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
