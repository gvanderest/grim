# `grim-actor`
> The "beings" of the world (characters, players, placement) and the being-reading command verbs.

**Role:** vertical — actors (beings + their verbs)
**Depends on:** `grim-core`, `grim-world`, `grim-networking`, `grim-color`

An **actor** is any entity that can act in the world and be placed in a room —
player characters and creatures (mobs). Every being carries the shared `Actor`
base (race/level/gender); a PC additionally carries a `Character` (account
state) and, *while connected*, a `Player`; a mob carries a `Creature` marker.
This crate owns those being components, the flat `StoredCharacter` disk DTO, and
the command handlers that read them. It sits strictly **above** the being-free
`grim-world`: it depends on `grim-world` (room topology + address lookups +
shutdown machinery + `RoomLocation`) and never the reverse.

Entity composition: online PC = `Name + Actor + Character + Player + InRoom`;
linkdead PC = `Name + Actor + Character + Linkdead + InRoom` (no `Player`);
creature = `Name + Actor + Creature + InRoom`. The display **name** lives in the
`Name` component (`grim_core::components::Name`), never on `Character`.

## Components

| Component | File | Purpose |
|---|---|---|
| `Actor` | `src/actor.rs` | Shared "alive thing" base carried by every being (PC + creature): `race`, `level`, `gender`. Movement/perception/WHO read build data here. |
| `Creature` | `src/actor.rs` | Marks a being as a non-player mob (replaces the former `grim_world::Npc`). |
| `Character` | `src/character.rs` | PC-only being belonging to an account; carries `id`, `account_id`, `created_at`, roles, `class`, `title`, `restrings`, `last_room`. No `name`/`race`/`level`/`gender` (→ `Name`/`Actor`). |
| `Player` | `src/player.rs` | Present **only while connected**; links to the live `Connection`. Absence (with `Character`) = linkdead. |
| `OutputHistory` | `src/player.rs` | Bounded ring buffer of recent output lines, for reconnect. |
| `Linkdead` | `src/player.rs` | Character is in-world but its player disconnected (no `Player`). |
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
| `EngineCommand` | Message (input, from `grim-core`) | each `src/commands/*.rs` |
| `InfoMessage` | Message (output, from `grim-core`) | `look`/`movement`/`title`/`shutdown` |
| `DisconnectRequest` | Message (from `grim-networking`) | `src/commands/quit.rs` |
| `LookRoom` / `LookEntity` / `MoveEvent` | Message (world-happening events, **registered by `grim_world::WorldPlugin`**) | emitted by `look`/`movement` |
| `ServerBroadcast` | Message (**registered by `grim_world::ShutdownPlugin`**) | `src/commands/shutdown.rs` |

## Types
Non-component types this crate defines.

| Type | Kind | File | Purpose |
|---|---|---|---|
| `Role` | serde enum (`Admin`) | `src/character.rs` | A privilege a character holds; stored in `Character.roles` and gates admin verbs. Not an ECS component. |
| `StoredCharacter` | serde struct | `src/stored.rs` | The flat on-disk DTO for a PC — the **only** serde surface. `into_components()`/`from_components()` bridge it to `Name + Actor + Character`. Keeps the pre-split JSON layout (every optional field `#[serde(default)]`) so old `data/characters/<name>.json` still loads. |

## Notes
- **One plugin:** `ActorPlugin` (`src/plugin.rs`) calls each command's
  `pub(crate) fn register(app)` — the per-command convention: one file per
  command under `src/commands/`, each owning its systems + the messages it
  registers. `commands.rs` and `lib.rs` are declarations + re-exports only.
- The world-happening events (`LookRoom`/`LookEntity`/`MoveEvent`) are owned by
  `grim_world::WorldPlugin`; the shutdown countdown/signal machinery by
  `grim_world::ShutdownPlugin`. A full stack composes those alongside
  `ActorPlugin` (see `GrimHeadlessPlugins`).
- **`Player` is present only while connected.** On disconnect, `save_on_disconnect`
  (grim-persistence) removes `Player` and inserts `Linkdead`; on reconnect the
  scene inserts `Player` and removes `Linkdead`. So online ⇔ has `Player`,
  linkdead ⇔ has `Character` and no `Player`. Takeover keeps the *live* `Player`
  (whose `connection` differs from the closing one) untouched.
- `Gender` stays in `grim-core` (the session `ClientState` references it);
  `Actor`/`StoredCharacter` point *up* at it. `RoomLocation` now lives in
  `grim-world` (`Character.last_room` and `StoredCharacter.last_room` point up at
  it). See ARCHITECTURE.md §4 and ADR-0001.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
