# `grim-combat`
> PVE combat: engage mobs, trade auto-attack rounds, kill for XP + coin + loot.

**Role:** vertical — combat
**Depends on:** `grim-core`, `grim-actor`, `grim-world`, `grim-object`, `grim-text`, `grim-target`, `grim-networking`, `grim-config`

Room-scoped `Combat` fights + per-being `Engaged` target lists (index 0 =
primary). `kill` engages + resolves the first round immediately; 5s rounds
thereafter, one auto-attack per being per round vs its primary. `flee`
success reads like a walk ("You flee to the …" + arrival `LookRoom`). Skills and
spells share `AbilityDef` (flagged `Skill`/`Spell`); `kick` is the seeded
universal skill. Corpses are one-way containers filled from the global loot
table; coin auto-awards. Output renders per-recipient in `grim-scene`.

## Components
| Component | File | Purpose |
|---|---|---|
| `Combat` | `src/engage.rs` | Live fight on a room entity; `members` while non-empty |
| `Corpse` | `src/corpse.rs` | Death timer on a corpse entity |
| `CorpseMarker` | `src/corpse.rs` | Disambiguator so corpse queries never collide with plain containers |
| `AttackNoun` | `src/engage.rs` | Creature unarmed noun (`bite` default); players always `punch`. Kicks render `kick`. |

Being-side state lives in `grim-actor` (`Health`, `Posture`, `Engaged`,
`CombatSlow` — see `combat_state.rs`); container markers (`Container`,
`OneWay`) live in `grim-object`.

## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|
| `handle_kill` | `Update` | `src/engage.rs` | Engage + immediate first strike; fail-closed on bad targets |
| `handle_switch` | `Update` | `src/engage.rs` | Reorder own targets (primary = index 0) |
| `prune` | `Update` | `src/engage.rs` | Drop stale room memberships; despawn empty fights |
| `handle_flee` | `Update` | `src/actions.rs` | Combat-only random-exit escape (50%, 3s slow) |
| `handle_kick` | `Update` | `src/actions.rs` | Class-granted skill, 1.5x strike, 6s cooldown, never initiates |
| `handle_cast` | `Update` | `src/actions.rs` | Spell path (no spells seeded; unknown-spell reply) |
| `tick_rounds` | `Update` | `src/rounds.rs` | 5s auto-attack rounds vs primaries |
| `tick_regen` | `Update` | `src/regen.rs` | 3s HP regen (1/2/4 by posture) |
| `tick_corpses` | `Update` | `src/death.rs` | Corpse timer/empty despawn |
| `handle_posture` | `Update` | `src/posture.rs` | `sit`/`sleep`/`stand` |
| `aggro_on_enter` | Observer (`Enter`) | `src/aggro.rs` | Aggressive mobs engage entering PCs with first strike now |
| `tick_slows` / `clear_slows` | `Update` | `src/posture.rs` | Flee/move delay decay + cleanup |
| `ensure_posture` | `Update` | `src/posture.rs` | Backfill `Posture` on old beings |

## Commands
Player-facing verbs and where to find their handlers.
| Command | Handler | Summary |
|---|---|---|
| `kill <target>` | `engage::handle_kill` | Engage + first strike now |
| `flee` | `actions::handle_flee` | Random exit, 50%, 3s slow either way |
| `switch <target>` | `engage::handle_switch` | Move a mutual target to primary |
| `kick [<target>]` | `actions::handle_kick` | Combat-only skill, defaults to primary |
| `cast <spell> [<target>]` | `actions::handle_cast` | Spell ability path |
| `sit` / `sleep` / `stand` | `posture::handle_posture` | Posture (regen ×1/×2/×4) |

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `Damaged` (nouns) / `Died` / `FightStart` / `Fled` (direction+dest) | Message (facts, rendered in `grim-scene`) | `src/events.rs` |
| `DamageKind` | value (`Strike`, `Kick`) | `src/events.rs` |
| `AbilityRegistry` / `AbilityCooldowns` / `CombatClock` | Resource | `src/ability.rs` |
| `GlobalLoot` | Resource | `src/loot.rs` |
| `RoundTimer` / `RegenTimer` / `CombatRng` | Resource | `src/state.rs` |

## Notes
- Formula inputs are levels only (`src/formula.rs` pure fns); #60 swaps inputs without touching the loop.
- Fact-only events (no `Attack` attempt yet); movement proves the pair pattern when vetoers arrive.
- Death: mobs → container-corpses (global-table rolls, despawn on empty/120s); PCs → respawn at `StartingRoom`, full heal, XP debt, inventory kept.
- `Health::pc()` is 100/100; wolves 20 HP; the Old Cave Bear is raid-scale (level 12, 4000 HP).
- PVP falls out naturally: verbs target any engaged being, not just creatures.
- Aggression is per-blueprint (`aggressive: bool`, default false); `Enter`-fact observer, same first-strike shape as `kill`.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
