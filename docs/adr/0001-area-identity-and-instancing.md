# Area identity, addressing, and instancing

status: accepted

We are moving areas/rooms from a single `Uuid`-keyed world toward a
**Blueprint → Instance** model with a four-tier identity scheme, so that areas can
eventually be instanced (multiple live copies) without ID collisions or ambiguous
targeting. This ADR records the identity model, the addressing/precedence rules, the
load procedure, and how a character's last location is persisted.

**Instancing itself is NOT being implemented now.** The goal here is only that the
identity model, persistence layout, and placement seams *allow* instancing later
without a rewrite — the design need not be perfectly aligned for it, just
non-blocking. Everything about live Instances (Instance ID generation, `instances/`
saves, on-demand loading, unload) is forward-plumbing. What gets built now is the
Canonical, non-instanced path: `Uuid` → Grim ID, `friendly_id` → Slug, the
`last_room`/`last_canonical_room` fields, and routing placement through one seam.

## Terminology

Defined in [CONTEXT.md](../../CONTEXT.md): **Blueprint** (on-disk definition),
**Instance** (live copy in the ECS), **Canonical** (the one linkage-target Instance
per Blueprint). The word **Template** is reserved for Catalog entries and is NOT used
for on-disk area definitions.

## Identity — four tiers, most to least specific

- **Entity ID** — Bevy `Entity`. Runtime-only, boot-local, never persisted. The
  truly-unique handle for one live thing.
- **Grim ID** — immutable identity of a persistent record (account, character, and a
  Blueprint's area + rooms). A `nanoid` over **base62** (`A-Za-z0-9`), **length 12**
  (~71 bits). Generated once, never changed. A room's Grim ID lives in its Blueprint,
  so every Instance stamped from it shares the same room Grim ID. Replaces the old
  `Uuid` `id` fields (no migration — reseed; data loss is acceptable at this stage).
- **Slug** — human-facing alias, usually the filename (`haven`, `market-square`).
  NOT unique, renamable, lower-case with hyphens. Base62 Grim IDs have no `-`/`_`, so
  a Grim ID never shares a shape with a Slug. Formerly "Friendly ID".
- **Instance ID** — distinguishes one non-Canonical Instance from another. Generated
  at instance creation, same shape as a Grim ID (base62, length 12, globally unique)
  but a different role: Grim ID is the shared record/Blueprint identity, Instance ID
  is this live copy's identity. Persisted — it is the `instances/<instance-id>.json`
  filename and the `last_room` area part. The Canonical Instance carries none (its
  Blueprint area Grim ID resolves to it); an instanced area holds both ids.

## Addressing (`goto` and similar)

A target room address is one of:

- an **Entity ID** (single boot-local token), or
- `<area>:<room>`, where each side is independently a Grim ID or a Slug (any
  combination); the area side may also be an Entity ID, or
- a bare room token (Grim ID or Slug).

**Resolution precedence** for an ambiguous token: `Entity ID → Grim ID → Slug`,
first hit wins.

- `:` present → treat as an `<area>:<room>` pair.
- All-digits → try as a live Entity; if it does not resolve to a live room entity,
  **fall through** (a Grim ID could in principle be all-digits) rather than hard-fail.
- Else exact **Grim ID** match (globally unique), then **Slug** (the fuzzy fallback).
- A **Slug that matches multiple Instances** resolves to the **Canonical** one.
  Targeting a specific non-Canonical Instance requires an Entity ID (or, later, an
  Instance ID) on the area side.
- Entity IDs are written/parsed as `Entity::to_bits()` decimal (round-trips exactly).

## Canonical flag + load procedure

- Each area Blueprint carries a boolean `canonical` flag. (A simple bool for now;
  multiplicity — "create N on startup" — is deferred.)
- Startup:
  1. Load all Blueprints from disk.
  2. For each `canonical: true`, stamp one **Canonical Instance** into the ECS.
  3. Resolve every exit ref to a target room **Entity**.
  4. Non-Canonical Instances are created later on demand; their *outbound* exits
     resolve to Canonical targets, but no Canonical exit ever points *into* an
     instanced copy (walking back lands in Canonical) until explicit instance-aware
     exit logic exists — not yet.

**Link storage rule:** Blueprint exits hold *building refs* (Slug or Grim ID, for
authoring convenience); **Instances hold `Entity` refs only**. Resolution happens at
load. Cross-area links bind **Canonical → Canonical** by Blueprint identity.

**Dangling exit:** if an exit's target cannot be resolved to a live Canonical
Instance (target Blueprint is `canonical: false`, or missing), **log an error and
skip creating that exit**. Never fail startup over one bad link — a work-in-progress
MUD always has half-wired areas.

## Character last location

Two persisted fields, each `(area_part, room_part)`. **Persisted references are Grim
IDs only** — never Slugs (Slugs are human-facing and renamable, so unsafe to store).

- `room_part` — always a room **Grim ID**.
- `last_room.area_part` — the **Instance ID** when the character is in an instanced
  area; otherwise the Canonical area's **Grim ID**. One field: the resolver tries the
  live-instance registry first, else treats it as a Canonical Grim ID.
- `last_canonical_room.area_part` — the Canonical area's **Grim ID** (always
  resolvable, no instance). Formerly `last_stable_room`.

**Update on move:** in an instanced area → update `last_room` only; in a Canonical
area → update **both** (there they coincide). **Until instancing exists, every room
is Canonical, so `last_room` and `last_canonical_room` are always equal** — they only
diverge once a character can stand in an instance.

**Resume fallback** (login / copyover):
`last_room` (live Instance by Instance ID → room by Grim ID) →
`last_canonical_room` (Canonical area + room Grim ID) → **StartingRoom**.

**One placement seam.** The location-field update is a property of the *destination*,
not of how the character arrived. Every path that places a character in a room —
walk, `summon`, `goto`, portal, login/resume, death-recall — routes through a single
placement primitive that sets `InRoom` and updates the location fields (Canonical
destination → both; instanced → `last_room` only). No command mutates those fields
directly, so `summon`-into-an-instance stays correct. (Today's `save_on_move` on
`MoveEvent` is the seed of this seam.)

**Instance IDs are never recycled.** They are generate-only; a deleted instance's id
is never reissued. This is what makes a stale `last_room` safe — it can only miss
(→ `last_canonical_room`), never resolve into a *different* later instance. Recycling
would let a player resume into an unrelated instance.

**Instance snapshots exclude players.** An instance persists its rooms + world
contents (items, mobs), not the characters standing in it — characters are their own
persisted records. So summoning a player into an instance cannot duplicate or trap
them when the instance saves/unloads.

**Today:** Instances are not recreated on startup/copyover (only Canonical ones are),
so post-reboot the `last_room` step always misses and players resume at
`last_canonical_room`. This is intended; `last_room` is forward-plumbing for when
instance persistence exists.

## Persistence asymmetry

- **Canonical areas are ephemeral.** They are reseeded from their Blueprint on every
  boot/copyover; runtime changes are discarded. (This is already how copyover behaves
  — the world reloads from scratch.)
- **Instances are persistent.** Saved under `instances/`, keyed by their **Instance
  ID** (area-level). Changes are durable until the instance is deleted. Once instance
  persistence exists, the boot/copyover successor reloads instances from disk, which
  is what makes the `last_room` resume step (Instance ID → room) actually resolve.

## Instance loading — lazy, on demand

Instances are **loaded on demand, when a character resolves into one** — never
eagerly. This mirrors lazy character loading.

- **Crash restart** → no instances loaded; an instance loads only when a character
  whose `last_room` points into it logs in.
- **Copyover** → many instances load at once, but only as a side effect of many
  characters resuming at once — the same code path, not a special case.
- An idle instance (no characters) stays on disk, unloaded.
- **Load-once:** keyed by Instance ID, so two characters resuming into the same
  instance trigger one load; the second finds it already live.

## Instance save = whole-area snapshot

An instance is saved as a **full snapshot** of the whole area —
`instances/<instance-id>.json` = `blueprint_id` + every room's current state +
contents — not a delta against the Blueprint. A snapshot keeps the instance
self-contained and frozen: a monster killed in the instance stays dead, independent
of later Blueprint edits. (Respawn behaviour is a separate, later problem.)

## Open questions (decide at implementation)

- **Instance unload timing** — when the last character leaves, save + unload
  immediately vs after a grace timer. Not critical; either works. The file stays on
  disk for a later on-demand reload; deletion is a separate, explicit act.
- **Instance write timing** — on-change / on-unload / periodic. Ties into the
  deferred durable-persistence work (WAL + dirty-flag/timer autosave).
- **Instance lifecycle** — what creates an instance, and idle-delete vs
  explicit-delete.
- **When an instance is written** — on-change / on-unload / timer. Ties into the
  deferred durable-persistence work (WAL + dirty-flag/timer autosave).
- **Instance lifecycle** — what creates one, and whether it is idle-deleted vs
  explicitly deleted.

## Consequences

- On-disk format and all `id` fields change type (`Uuid` → base62 Grim ID). Existing
  saved data is discarded, not migrated.
- Slugs must never leak into persisted record-to-record references; only into
  Blueprint authoring and human display.
- Instance targeting/persistence is deliberately unfinished — the model reserves the
  seams (Instance ID, the `last_room` instance path) so adding it later is additive,
  not a rewrite.
