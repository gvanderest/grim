# `grim-engine-types`
> Shared primitive types — components, game events, and value types the engine's plugins agree on.

**Role:** horizontal (primitives)
**Depends on:** `grim-color` (re-exported), plus `bevy`, `serde`, `nanoid`, `chrono`.

## Components
| Component | File | Purpose |
|---|---|---|
| `Client` | `src/components.rs` | Session state machine, one per connection (holds `ClientState`, account/character links, input queue, cooldown). |
| `Account` | `src/components.rs` | Persisted account: identifier, password hash, owned character IDs. |
| `Character` | `src/components.rs` | Persisted character: name, roles, gender/race/class, level, title, restrings, last room. |
| `Area` | `src/components.rs` | An area — a collection of rooms. |
| `Room` | `src/components.rs` | A room: name, description, owning area, exits. |
| `Exits` | `src/components.rs` | `HashMap<Cardinal, Entity>` of a room's exits. |
| `InRoom` | `src/components.rs` | Which room an entity is currently in. |
| `Name` | `src/components.rs` | Display name for any visible entity. |
| `Description` | `src/components.rs` | Long description shown by `look <target>`. |
| `Player` | `src/components.rs` | Marks a character player-controlled; links to its `Connection` (`None` = linkdead). |
| `OutputHistory` | `src/components.rs` | Bounded ring buffer of recent output lines. |
| `Linkdead` | `src/components.rs` | Marker: character still in-world but player disconnected. |

## Systems
None. This crate is type definitions only — no `Plugin`, no `add_systems`, no observers.

## Commands
None. The `Command` enum (the closed set of player verbs) is *defined* here in `src/events.rs`, but its handlers live in the consuming plugins (`grim-scene`, `grim-world`, `grim-channel`). The enum is documented as a defect slated for retirement in favour of per-plugin event types (ARCHITECTURE.md §5.2, §8).

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `StartingRoom` | Resource | `src/components.rs` |
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
