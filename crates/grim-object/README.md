# `grim-object`
> Things beings can pick up, carry, and drop.

**Role:** vertical — things
**Depends on:** `grim-core`, `grim-actor`, `grim-text`

An object is `Object + Name + Keywords + RoomDescription (+ InRoom while on the
ground, + CarriedBy while carried — never both)`. `Name` is the short name
(inventory rows, pickup lines); `RoomDescription` is the long line under the
room description; `Keywords` feeds get/drop matching with `look`'s ranking.

## Components
| Component | File | Purpose |
|---|---|---|
| `Object` | `src/object.rs` | Marker for a pickable thing. |
| `CarriedBy { carrier }` | `src/object.rs` | Carrier link; present instead of `InRoom`, never alongside it. |

## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|
| `handle_get` | `Update` | `src/commands/get.rs` | Reads `Command::Get`; swaps `InRoom`→`CarriedBy`, emits `ItemEvent::Pickup`, else a "not here" `InfoMessage`. |
| `handle_drop` | `Update` | `src/commands/get.rs` | Reads `Command::Drop`; swaps `CarriedBy`→`InRoom`, emits `ItemEvent::Drop`, else a "not carrying" `InfoMessage`. |
| `handle_inventory` | `Update` | `src/commands/inventory.rs` | Reads `Command::Inventory`; emits an `InfoMessage` listing carried shorts (sorted) or the empty line. |

## Commands
Player-facing verbs and where to find their handlers.
| Command | Handler | Summary |
|---|---|---|
| `get <keyword>` | `get::handle_get` | Pick up the best-matching ground object (exact name, exact keyword, shortest-prefix). |
| `drop <keyword>` | `get::handle_drop` | Drop the best-matching carried object into the room. |
| `inventory` | `inventory::handle_inventory` | List carried short names. |

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `ItemEvent` | Message (emitted; `grim-core`) | `src/commands/get.rs` |
| `EngineCommand` | Message (consumed) | `src/commands/get.rs`, `src/commands/inventory.rs` |
| `InfoMessage` | Message (emitted) | `src/commands/get.rs`, `src/commands/inventory.rs` |

## Notes
- Rendering is per-recipient in `grim-scene` (`format_item_events`): the actor sees "You pick up …", the rest of the room "<name> picks up …".
- Room listings show ground objects' `RoomDescription` under the creatures (`grim-scene`).
- Carried objects are memory-only: nothing persists, so a reboot respawns the seeded objects and empties every inventory.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
