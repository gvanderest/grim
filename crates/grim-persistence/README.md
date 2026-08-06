# `grim-persistence`
> Loads accounts/characters from disk and mirrors world state back as fire-and-forget JSON writes.

**Role:** horizontal (infrastructure)
**Depends on:** `grim-core`, `grim-world`, `grim-actor`, `grim-networking`

## Components
| Component | File | Purpose |
|---|---|---|
| None | — | Reads/writes existing domain components (`Account`, `Character`, …); defines none of its own. |

## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|
| `load_persisted_data` | `Startup` | `src/persistence.rs` | Spawn every account found on disk. Characters are NOT loaded here — they load lazily at login. Missing dirs treated as empty. |
| `save_on_disconnect` | `Update` | `src/persistence.rs` | On `ConnectionClosed`: persist bound account + character (refreshing `last_room` from `InRoom`), transfer `OutputHistory`, mark the character `Linkdead`, despawn client/connection. |
| `save_on_move` | `Update` | `src/persistence.rs` | On `MoveEvent` for a character: write the character JSON so on-disk `last_room` stays current for copyover restore. |

Helper functions (not systems): `load_account_characters` and `load_character_by_name` read characters lazily for login-time loading.

## Commands
Player-facing verbs and where to find their handlers.
| Command | Handler | Summary |
|---|---|---|
| None | — | This crate registers no player commands. |

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `PersistenceConfig` | Resource | `src/persistence.rs` |
| `ConnectionClosed` | Message (consumed; from `grim-networking`) | `src/persistence.rs` |
| `MoveEvent` | Message (consumed; from `grim-core`) | `src/persistence.rs` |
| `LinkdeadAnnounce` | Message (emitted; from `grim-core`) | `src/persistence.rs` |

## Notes
- Writes are **fire-and-forget** (`fs::write`, errors ignored) — no WAL, no autosave, no transactionality yet.
- `PersistenceConfig.dir` (default `data/`) redirects both load and save; a test harness or author inserts a custom dir before the plugin initialises to isolate state.
- Characters load lazily (at login) and despawn on quit, so the world holds only in-play characters; startup loads accounts only.
- Character files key on canonical `name` (`<name>.json`); accounts key on `id`.
- Evolving toward pluggable storage drivers / durable persistence (WAL + autosave); see the durable-persistence follow-up.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
