# `grim-engine-types`
> Shared primitive types — components, game events, and value types the engine's plugins agree on.

**Role:** horizontal (primitives)
**Depends on:** `grim-color` (re-exported), plus `bevy`, `serde`, `nanoid`, `chrono`.

## Components
Only the primitives this crate still owns. The being components
(`Character`/`Actor`/`Creature`/`Player`/`InRoom`/`OutputHistory`/`Linkdead`) live
in **`grim-actor`**, and the world-topology components (`Area`/`Room`/`Exits`) plus
`RoomLocation` live in **`grim-world`** — see those crates' READMEs.

| Component | File | Purpose |
|---|---|---|
| `Client` | `src/components.rs` | Session state machine, one per connection (holds `ClientState`, account/character links, input queue, cooldown). |
| `Account` | `src/components.rs` | Persisted account: identifier, password hash, owned character IDs. |
| `Name` | `src/components.rs` | Display name for any visible entity (a being's name lives here, not on `Character`). |
| `Description` | `src/components.rs` | Long description shown by `look <target>`. |

## Systems
None. This crate is type definitions only — no `Plugin`, no `add_systems`, no observers.

## Commands
This crate *defines* the `Command` enum (the closed set of player verbs, `src/events.rs`) — the vocabulary only; it registers no handlers. The live `grim::CommandRegistry<Command>` **resource is owned by `grim-scene`** (`parser::command_registry()`, inserted in its `ScenePlugin`), which parses input into `EngineCommand`. Handlers live in the owning gameplay crates — see their READMEs for the per-command file:

| Command(s) | Handler crate → file |
|---|---|
| `look` / `move` / `goto` / `quit` / `title` / `shutdown` | `grim-actor` → `src/commands/<name>.rs` |
| `say` / `yell` / `ooc` / `tell` / `reply` / `gecho` | `grim-channel` → `src/channel.rs` |
| login / account-creation / character-select verbs | `grim-scene` (session state machine) |

The closed `Command` enum + last-registered-wins registry are documented as defects slated for per-plugin typed dispatch (ARCHITECTURE.md §5.2, §8).

## Resources & Events
This crate owns no `Resource`s — `StartingRoom` moved to `grim-world` in Placement Phase 2a. The events are all `Message` types in `src/events.rs`:

| Name | Kind | File |
|---|---|---|
| `EngineCommand` | Message | `src/events.rs` |
| `LookRoom` | Message | `src/events.rs` |
| `LookEntity` | Message | `src/events.rs` |
| `SayEvent` | Message | `src/events.rs` |
| `YellEvent` | Message | `src/events.rs` |
| `OocEvent` | Message | `src/events.rs` |
| `GlobalEcho` | Message | `src/events.rs` |
| `MoveEvent` | Message | `src/events.rs` |
| `InfoMessage` | Message | `src/events.rs` |
| `LoginAnnounce` | Message | `src/events.rs` |
| `LogoutAnnounce` | Message | `src/events.rs` |
| `LinkdeadAnnounce` | Message | `src/events.rs` |
| `ServerBroadcast` | Message | `src/events.rs` |

## Notes
- Also defines non-ECS value types: `Command` and `ClientState` (`src/events.rs` / `src/components.rs`), `Gender` (`src/character.rs`), `Cardinal` (`src/cardinal.rs`), and `GrimId` (base62 ×12 id, `src/id.rs`). (`RoomLocation` moved to `grim-world` in Placement Phase 2a step 3.) A `prelude` re-exports the common set; `src/color.rs` re-exports `grim-color` so `grim::color::*` keeps resolving.
- The events named `SayEvent`/`MoveEvent`/etc. are **facts only** — there is no attempt/fact split yet, so nothing can veto them (ARCHITECTURE.md §6, §8).
- Called a "god-types crate" in ARCHITECTURE.md §8: colour, `tr`, the command registry, and wire types have already been split out; game events + components are what remain. Expect this crate to shrink over time — improve over time.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
