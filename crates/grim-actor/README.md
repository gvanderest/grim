# `grim-actor`
> The "beings" of the world (characters, players, placement) and the being-reading command verbs.

**Role:** vertical — actors (beings + their verbs)
**Depends on:** `grim-engine-types`, `grim-world`, `grim-networking`, `grim-color`

An **actor** is any entity that can act in the world and be placed in a room —
player characters today, mobs/NPCs later. This crate owns the being components
and the command handlers that read them. It sits strictly **above** the
being-free `grim-world`: it depends on `grim-world` (room topology + address
lookups + shutdown machinery) and never the reverse.

## Components

| Component | File | Purpose |
|---|---|---|
| `Character` | `src/character.rs` | A persisted being belonging to an account; carries name, roles, build, title, restrings, `last_room`. |
| `Player` | `src/player.rs` | Marks a character player-controlled; links to its `Connection` (`None` = linkdead). |
| `OutputHistory` | `src/player.rs` | Bounded ring buffer of recent output lines, for reconnect. |
| `Linkdead` | `src/player.rs` | Character is in-world but its player disconnected. |
| `InRoom` | `src/placement.rs` | Which room an actor currently stands in. |

## Systems

| System | Schedule | File | Purpose |
|---|---|---|---|
| `look::handle_look` | `Update` | `src/commands/look.rs` | Reads `Command::Look`; emits `LookRoom` (no target) or `LookEntity`, else a "not here" `InfoMessage`. |
| `movement::handle_move` | `Update` | `src/commands/movement.rs` | Reads `Command::Move`; walks an exit, refreshes `last_room`, emits `MoveEvent` + auto-look. |
| `movement::handle_goto` | `Update` | `src/commands/movement.rs` | Admin teleport to a room by address (entity/grim id/slug, `area:room`). |
| `quit::handle_quit` | `Update` | `src/commands/quit.rs` | Reads `Command::Quit`; emits `DisconnectRequest` for the player's connection. |
| `title::handle_title` | `Update` | `src/commands/title.rs` | Reads `Command::Title`; sets/clears the actor's title (≤60 chars). |
| `shutdown::handle_shutdown_command` | `Update` (`grim_world::ShutdownSet::Command`) | `src/commands/shutdown.rs` | Reads `Command::Shutdown`; admin-gates the graceful countdown (state/tick stay in `grim-world`). |

## Commands
Player-facing verbs and where to find their handlers.

| Command | Handler | Summary |
|---|---|---|
| `look [target]` | `src/commands/look.rs` | Describe the current room, or a named entity within it. |
| `move` — `n`/`e`/`s`/`w`/`u`/`d` (+ `north`…) | `src/commands/movement.rs` | Walk through an exit; emits `MoveEvent`. Direction aliases parsed in `grim-scene`. |
| `goto <address>` | `src/commands/movement.rs` | Admin teleport to a room by address. |
| `quit` | `src/commands/quit.rs` | Request a clean disconnect (save + despawn happen in `grim-scene`). |
| `title [text]` | `src/commands/title.rs` | Set (or, bare, clear) the actor's title; rejected over 60 chars. |
| `shutdown <seconds>` | `src/commands/shutdown.rs` | Admin-only graceful server-shutdown countdown. |

## Resources & Events

| Name | Kind (Resource/Message) | File |
|---|---|---|
| `EngineCommand` | Message (input, from `grim-engine-types`) | each `src/commands/*.rs` |
| `InfoMessage` | Message (output, from `grim-engine-types`) | `look`/`movement`/`title`/`shutdown` |
| `DisconnectRequest` | Message (from `grim-networking`) | `src/commands/quit.rs` |
| `LookRoom` / `LookEntity` / `MoveEvent` | Message (world-happening events, **registered by `grim_world::WorldPlugin`**) | emitted by `look`/`movement` |
| `ServerBroadcast` | Message (**registered by `grim_world::ShutdownPlugin`**) | `src/commands/shutdown.rs` |

## Types
Non-component types this crate defines.

| Type | Kind | File | Purpose |
|---|---|---|---|
| `Role` | serde enum (`Admin`) | `src/character.rs` | A privilege a character holds; stored in `Character.roles` and gates admin verbs. Not an ECS component. |

## Notes
- **One plugin:** `ActorPlugin` (`src/plugin.rs`) calls each command's
  `pub(crate) fn register(app)` — the per-command convention: one file per
  command under `src/commands/`, each owning its systems + the messages it
  registers. `commands.rs` and `lib.rs` are declarations + re-exports only.
- The world-happening events (`LookRoom`/`LookEntity`/`MoveEvent`) are owned by
  `grim_world::WorldPlugin`; the shutdown countdown/signal machinery by
  `grim_world::ShutdownPlugin`. A full stack composes those alongside
  `ActorPlugin` (see `GrimHeadlessPlugins`).
- `Gender` and `RoomLocation` stay in `grim-engine-types` (the session
  `ClientState` references `Gender`, and hoisting `RoomLocation` would cycle);
  `Character` points *up* at them. See ARCHITECTURE.md §4 and ADR-0001.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
