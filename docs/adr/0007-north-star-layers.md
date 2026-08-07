# New layers implied by the north-star scenarios

status: Proposed

> ⚠️ **Needs maintainer decision — do not implement until accepted.** This ADR
> is a design proposal for the deferred §8/§9 redesigns in
> [ARCHITECTURE.md](../ARCHITECTURE.md). It records recommended crate boundaries
> and dependency directions for review; no code should change on its basis until
> it is marked `accepted`. This ADR **sketches boundaries only** — each layer
> gets its own detailed ADR when it is actually built.

The north-star scenarios that drove the crate-placement work (grim-map, combat,
crafting, and a player-config command) each imply a layer that does not exist yet.
This ADR sketches those layers, their dependency directions, and — most
importantly — **which are needed soon versus speculative**, so the acyclic
dependency model of ARCHITECTURE.md §4 stays intact as they land. Per §3, none of
these is created until its second-implementation / real-consumer threshold is met;
this ADR only reserves the shape.

## Terminology

Uses the §4 layering vocabulary: the acyclic spine is
`grim-color/grim-text → grim-command/grim-networking/grim-scene →
grim-world → grim-actor → verticals`. "Below `grim-world`" means a shared contract
that world-level and being-level code both depend on; "vertical" means a gameplay
plugin (combat, crafting) at or above the actor layer. New crates follow the
`grim-<system>` naming of §3.

## The proposed layers

### 1. `grim-skills` — abilities/kits tied to race & class

**Why:** ADR-0002 seeds race and class as registered data but nothing hangs
abilities off them. Both combat and non-combat gameplay (a warrior's `bash`, a
thief's `pick`, a crafter's `smith`) want abilities keyed to race/class, and the
combat north-star needs spells/abilities that keep working across a future
`grim-combat` engine swap (§9, "Combat contract split").

**Boundary & deps:** a contract crate holding `Ability`/`Kit` definitions
(registered data, like `RaceRegistry`/`ClassRegistry`) and the events an ability
raises. Depends on **`grim-actor`** (abilities act on beings) and reads the
race/class registries in **`grim-world`**. Combat and crafting depend on
`grim-skills`, **not the reverse** — so an ability can deal damage or consume a
material without knowing which combat/crafting engine is installed (the §9 payoff:
"spells and items keep working across it").

**Verdict: needed-soon-ish, but *after* combat starts.** A skills contract with no
consumer is the speculative-extension-point trap §3 warns against. Build it when
the *second* consumer appears (combat **and** a non-combat ability), which is the
threshold that makes the shared contract fit both. Until then, a single vertical
holds its own abilities.

### 2. `grim-item` — shared item components for loot/harvest/craft

**Why:** the crafting north-star (harvest → material → craft → product) and the
combat north-star (loot drops) both move **items**. If combat, crafting, and a
future shop each define their own item type they will not interoperate — a
harvested material could not be a craft input, a looted sword could not be
equipped.

**Boundary & deps:** a contract crate holding the shared item components
(`Item`, stack/quantity, the `Container` relationship) and item events (pick
up/drop/transfer). Sits **at or just below the actor layer** — items exist in
rooms (world) and in inventories (actor), so it depends on **`grim-world`** for
room placement and is depended on by **`grim-actor`** (inventory) and every
vertical that moves items. It must **not** depend on combat or crafting — those
depend on it.

**Verdict: needed-soon.** It is the shared vocabulary the crafting and combat
north-stars both require; it is the analogue of `grim-actor` (a shared being
contract) for things. Build the contract early even though the first vertical is
small — items are the interop point, and retrofitting a shared item type after two
verticals each rolled their own is the expensive path.

### 3. `grim-input` — only if the scene/render rework needs it

**Why:** ADR-0003 (scene stack) and ADR-0005 (render pipeline) may push input
parsing (line editing, per-transport framing quirks, telnet vs websocket
differences) past what belongs inside `grim-scene`.

**Boundary & deps:** would hold the input-parsing/framing seam, depended on by
`grim-scene`. Depends on `grim-networking` for the raw line.

**Verdict: speculative — do not create now.** §3 is explicit: split on the second
implementation, and a single input path is not one. Keep parsing in `grim-scene`
(ADR-0003) and `grim-networking-telnet` until a concrete second case (a websocket
client with genuinely different framing needs) forces the split. Named here only
so it is not rediscovered as a surprise.

### 4. Player config registry — a peer of restrings

**Why:** the config north-star is a player-facing `config` command that toggles
typed settings (colour on/off, brief-mode, auto-loot, prompt format). Plugins must
be able to **register their own settings** with a type, a default, and a scope
(account-wide or per-character), the same way channels register toggles (§7) and
ADR-0002's registries hold race/class.

**Boundary & deps:** a registry resource of `SettingDef { key, type, default,
scope }` plus a single `config` command that lists/sets them, mirroring the
`RaceRegistry`/`ReservedNamePrefixes` `init_resource` pattern (ADR-0002). Persisted
state (the chosen values) is **account-or-character-scoped and owned by
`grim-persistence`** — exactly like player aliases and channel toggles already are
(§4 crate map). So the registry/command can live in `grim-scene` or a small
`grim-config` crate; the *stored values* live with persistence. It is a **peer of
restrings** (a per-character customization), not a new subsystem tier.

**Verdict: needed-soon and cheap.** It is a small, self-contained feature with an
obvious pattern to copy (channel toggles + restrings + seeded registries). Likely
its own tiny `grim-config` crate, or folded into `grim-scene` if it stays small —
a crate-boundary call for the maintainer. Persistence edge:
`grim-persistence` already owns aliases and channel toggles, so config values join
that same save surface with no new persistence concept.

### 5. Reusable container open/close command pattern

**Why:** chests, corpses, bags, and doors all share an open/close/look-inside
interaction. The crafting and combat north-stars both hit it (a crafting station, a
lootable corpse). Re-implementing it per vertical guarantees drift.

**Boundary & deps:** **not a crate** — a reusable *pattern/command family* over the
`Container` component that lives in **`grim-item`** (layer 2). `open`/`close`/`look
in` resolve against any entity carrying `Container`, so a corpse (combat) and a
chest (world) and a station (crafting) all reuse one command family. This is the
same "one mechanism, many registrations" stance as channels (§7): open/close is
configuration over a shared component, not N implementations.

**Verdict: needed-soon, rides `grim-item`.** It is a consequence of having a shared
`Container`, not a separate decision — ship it with `grim-item`.

## Dependency directions (must stay acyclic — §4)

```
grim-world ──> grim-item ──> grim-actor ──> grim-skills ──> grim-combat
                  │                              │              │
                  └──────────────> grim-crafting <─────────────┘
grim-persistence ──(owns stored values)──> config settings, aliases, channel toggles
grim-config (or in grim-scene) ──> grim-persistence   [values]  + grim-scene [command]
```

Rules that keep it acyclic:

- **Contracts point down, verticals point up.** `grim-item` and `grim-skills` are
  contracts; `grim-combat`/`grim-crafting` depend on them, never the reverse — the
  §9 combat-contract-split rationale generalised.
- **Nothing depends back on the facade** (§4, step 9) — new crates depend on
  subsystems, and `grim` re-exports them.
- **Persistence owns stored player state**, everywhere — config values, aliases,
  channel toggles — so no vertical grows its own save path.

## Needed-soon vs speculative — summary

| Layer | Verdict | Threshold to build |
|---|---|---|
| `grim-item` (+ container pattern) | **soon** | first vertical that moves items |
| Player config registry | **soon** | the `config` north-star; cheap, pattern exists |
| `grim-skills` | soon-ish | second ability consumer (combat **and** non-combat) |
| `grim-combat` / `grim-crafting` | when built | their own ADRs; split contract on 2nd engine (§9) |
| `grim-input` | **speculative** | a real second input-framing case; do not pre-create |

## Consequences

- The acyclic §4 spine extends cleanly: `grim-item`/`grim-skills` are contracts
  below their verticals, config values join the existing persistence surface, and
  no new crate depends on the facade.
- Each "soon" layer still waits for its real consumer (§3) — this ADR reserves
  shape and direction, it does not authorise creating empty crates.
- **Open questions for the maintainer:**
  - Does the player config registry warrant its own `grim-config` crate, or fold
    into `grim-scene` until it grows? (Recommend: fold in, split on growth.)
  - Is `grim-item` a contract crate from day one, or does the first item-moving
    vertical hold items until a second appears (per §3)? (Recommend: contract from
    day one — items are the interop point, unlike a single input path.)
  - Confirm `grim-skills` waits for a **non-combat** ability consumer before it is
    carved out, rather than being created alongside the first combat vertical.
