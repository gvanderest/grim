# `example-mud`
> The vanilla GRIM game: the one binary that composes the plugins and seeds the world.

**Role:** vertical — example game (the workspace's only binary)
**Depends on:** `grim`

## Components
| Component | File | Purpose |
|---|---|---|
| None | — | Uses the engine's components via `grim::prelude`; defines none of its own. |

## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|
| `seed::seed_world` | `Startup` | `src/seed.rs` | Read area blueprints (`data/areas/*.json`) from the filesystem, spawn every `canonical` area's rooms/exits/NPCs, and set `StartingRoom`. Panics if no starting room resolves. |

## Commands
Player-facing verbs and where to find their handlers.
| Command | Handler | Summary |
|---|---|---|
| None | — | The binary registers no commands directly; all verbs come from the `grim` subsystem crates. |

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `AreaBlueprintDir` | Resource | `src/seed.rs` |

## Binaries
| Binary | File | Purpose |
|---|---|---|
| `example-mud` (default-run) | `src/main.rs` | The MUD server: `MinimalPlugins` + `LogPlugin` + `GrimDefaultPlugins { telnet_port: 4000 }`, seeded with `seed::seed_world`. |
| `copyover_fixture` | `src/bin/copyover_fixture.rs` | Lightest real server for the copyover integration test; port / data dir / areas dir come from env (`GRIM_TEST_PORT`, `GRIM_TEST_DATA`, `GRIM_AREAS_DIR`) so the test isolates state and re-execs itself on `SIGUSR2`. |

## Tests
| Test | File | Purpose |
|---|---|---|
| dependency-direction guard | `tests/dep_direction.rs` | Asserts set-equality of the intra-workspace crate dependency graph against the allow-list from `docs/ARCHITECTURE.md`. |
| end-to-end scenarios | `tests/scenarios.rs` (+ `tests/harness/mod.rs`) | Boots the real world seed through the headless harness (`GrimHeadlessPlugins`, no transport) and drives it as a telnet user would. |
| copyover | `tests/copyover.rs` | Spawns the real `copyover_fixture`, triggers a copyover, asserts the same TCP connection survives and resumes in place. Unix-only. |

## Notes
- The world seed is exposed as a **library** (`src/lib.rs` → `seed`) so the test harness boots the *same* world the binary ships.
- Area/room content lives in `data/areas/*.json` (repo root), not in code — edit and restart, no recompile. References use stable `GrimId`s, not slugs; only `canonical` areas load at startup (see `docs/adr/0001-area-identity-and-instancing.md`).
- `main.rs` composes plugins + seeds; `seed.rs` reads blueprints; `src/bin/copyover_fixture.rs` is the copyover test fixture.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
