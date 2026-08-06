# `grim-world`
> World topology (areas, rooms, exits), movement, `look`, and server-control commands (`shutdown` + SIGUSR1 graceful bridge).

**Role:** vertical — world topology & movement
**Depends on:** `grim-engine-types`, `grim-networking`, `grim-color`

## Components
| Component | File | Purpose |
|---|---|---|
| `Area` | `src/world/topology.rs` | A named region grouping rooms (yell scope). |
| `Room` | `src/world/topology.rs` | A single location; carries its owning `area`. |
| `Exits` | `src/world/topology.rs` | Directional links from a room to adjacent rooms. |
| `Npc` | `src/npc.rs` | Marker for non-player characters. |

## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|
| `look::handle_look` | `Update` | `src/world/look.rs` | Reads `Command::Look`; emits `LookRoom` (no target) or `LookEntity`. |
| `movement::handle_move` | `Update` | `src/world/movement.rs` | Reads `Command::Move`; relocates the actor and emits `MoveEvent`. |
| `movement::handle_goto` | `Update` | `src/world/movement.rs` | Admin teleport to a room/entity by address. |
| `handle_quit` | `Update` | `src/world/mod.rs` | Reads `Command::Quit`; emits `DisconnectRequest`. |
| `title::handle_title` | `Update` | `src/world/title.rs` | Reads `Command::Title`; sets/clears a character's title (≤60 chars). |
| `handle_shutdown_command` | `Update` | `src/shutdown.rs` | Reads `Command::Shutdown`; starts/updates the countdown (admin-gated). |
| `install_signal_handler` | `Startup` | `src/shutdown.rs` | Installs the SIGUSR1 handler feeding graceful shutdown. |
| `tick_shutdown` / signal poll | `Update` | `src/shutdown.rs` | Advances `ActiveShutdown`, broadcasts warnings, emits `AppExit` at zero. |

## Commands
Player-facing verbs and where to find their handlers.
| Command | Handler | Summary |
|---|---|---|
| `look [target]` | `look::handle_look` (`src/world/look.rs`) | Describe the current room, or a named entity/exit within it. |
| `move` — `n`/`e`/`s`/`w`/`u`/`d` (+ `north`…) | `movement::handle_move` (`src/world/movement.rs`) | Move through an exit; emits `MoveEvent`. Direction aliases parsed in `grim-scene`. |
| `goto <target>` | `movement::handle_goto` (`src/world/movement.rs`) | Admin teleport to a room or entity. |
| `quit` | `handle_quit` (`src/world/mod.rs`) | Request a clean disconnect (save + despawn happens in `grim-scene`). |
| `title [text]` | `title::handle_title` (`src/world/title.rs`) | Set (or, bare, clear) the character's title; rejected over 60 chars. |
| `shutdown [seconds]` | `handle_shutdown_command` (`src/shutdown.rs`) | Admin-only server shutdown countdown (default 30s). |

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `StartingRoom(Entity)` | Resource | `src/world/topology.rs` |
| `ActiveShutdown(ShutdownCountdown)` | Resource | `src/shutdown.rs` |
| `ShutdownSignal` | Resource (private; SIGUSR1 flag) | `src/shutdown.rs` |
| `LookRoom` | Message (defined in `grim-engine-types`) | `src/world/look.rs` |
| `LookEntity` | Message | `src/world/look.rs` |
| `MoveEvent` | Message | `src/world/movement.rs` |
| `InfoMessage` | Message (output) | `src/world/mod.rs` |
| `ServerBroadcast` | Message (shutdown warnings) | `src/shutdown.rs` |
| `DisconnectRequest` | Message (from `grim-networking`) | `src/world/mod.rs` |

## Notes
- Two plugins: `WorldPlugin` (topology, movement, look, quit, title) and `ShutdownPlugin` (shutdown command + signal bridge).
- Topology types (`Area`/`Room`/`Exits`/`StartingRoom`) are hoisted to the crate root — consumers use `grim_world::{Area, Room, Exits, StartingRoom}` (Placement Phase 2a).
- Also owns the race/class registry content (`RaceRegistry`, `ClassRegistry`) read by `grim-scene`'s creation flow.
- Formatting lives in the plugin that owns the event (ARCHITECTURE.md §5.4): `look`/`move` emit intent events; rendering happens behind `grim-scene`'s renderer.
- Graceful shutdown: SIGUSR1 or the `shutdown` command both drive `ActiveShutdown`; the copyover flow (see MEMORY) relies on this.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
