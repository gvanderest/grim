# Per-plugin render pipeline

status: Proposed

> ⚠️ **Needs maintainer decision — do not implement until accepted.** This ADR
> is a design proposal for one of the deferred §8 redesigns in
> [ARCHITECTURE.md](../ARCHITECTURE.md). It records a recommended direction for
> review; no code should change on its basis until it is marked `accepted`.

`grim-scene` still owns a central `format_output`/formatter that turns game
results into player-facing lines (ARCHITECTURE.md §8; AGENTS.md lists `grim-scene`
as owning "output formatting"). ARCHITECTURE.md §5.4 already states the target
principle — **"formatting lives in the plugin that owns the event"** — because a
central formatter "would have to `use` every event type in the engine, so every
third-party command would require editing a crate the author does not own." This
ADR chooses *how* per-plugin rendering is structured so that composite output (a
room `look`: description + exits + occupants + items, each owned by a different
plugin) can still be assembled and, per the grim-map north-star, **filtered and
reordered**.

## Terminology

Defined in [CONTEXT.md](../../CONTEXT.md): **Catalog** (all author-facing text by
**Key**), **Colour code** (transport-independent markup), **Output policy**
(Pass/Buffer/Drop, §5.3). This ADR adds **data model** (a structured, not-yet-text
description of what a command produced) and **render pass** (a function that turns
part of a data model into Catalog-keyed lines).

## Problem with a central formatter

- **It must know every event type.** A single `format_output` `match`es on result
  types, so a third-party command's output requires editing `grim-scene` — the
  exact closed-crate failure §5.4 rejects, mirroring the `Command` enum (ADR-0004)
  and `ClientState` (ADR-0003).
- **Composite output has no seam.** A room `look` is assembled from several
  plugins' contributions (world owns the description and exits; the actor layer
  owns who is standing there; a future `grim-item` owns floor items). A central
  formatter hard-codes that assembly, so authors cannot reorder the room display
  or filter parts of it — the grim-map north-star ("`look` builds a data model,
  render is filterable passes") is unreachable.
- **Perception can't hook in.** The deferred `grim-perception` (§9) needs
  rendering to be **per-recipient** (`can_perceive`/`name_for`); a
  format-once-broadcast formatter structurally cannot do that.

## Options

### Option A — per-plugin formatter registration

Dissolve the central formatter into per-plugin formatters: each plugin registers a
formatter keyed by its event type; the dispatcher calls the one matching the event
it just produced. This is the literal reading of §5.4 ("`grim-world` owns both
`Look` and the observer that renders it").

- **+** Smallest step from §5.4's wording; each plugin renders its own event.
- **+** Open — no engine crate `use`s foreign event types.
- **−** No structure *within* a composite. `Look`'s formatter still hard-codes
  "description, then exits, then occupants," so the grim-map goal of a **filterable,
  reorderable** room display is not met — it just moves the monolith into one
  plugin's formatter.
- **−** Perception filtering must be re-implemented inside every formatter.

### Option B — data-model → ordered render passes (RECOMMENDED)

A command's observer builds a **data model** (a struct describing what happened /
what is present), not text. Rendering is a sequence of **render passes**, each
owned by the plugin responsible for that slice, each contributing Catalog-keyed
lines. For `look`: world contributes the description + exits passes, the actor
layer contributes the occupants pass, a future `grim-item` contributes the
floor-items pass. Passes are **ordered and filterable**, so an author reorders the
room display by reordering passes and a viewer's perception filters which passes
run and how names resolve.

- **+** Directly realises the grim-map north-star: model first, render is
  composable passes.
- **+** Open by construction — a plugin adds a pass without touching others.
- **+** The natural hook for `grim-perception` (§9): a pass runs per recipient and
  consults `can_perceive`/`name_for`, so combat/social/movement share one
  visibility mechanism instead of three.
- **+** Templates-with-control-flow (§9) become "a richer pass," not a new
  concept — the pass is where "for each exit, render a row" lives, replacing the
  Rust-side loop authors cannot reorder.
- **−** More machinery than A: a data model per composite command, a pass
  registry, and an ordering. Overkill for a command whose output is a single line.
- **−** Requires a home for the pipeline and the ordering rules (below).

### Option C — keep the central formatter, add extension callbacks

Leave `format_output` central but let plugins register callbacks it invokes.

- **−** The central crate still `use`s the shared surface and owns assembly order;
  callbacks are Option A wearing Option-B's costs. Rejected as a non-improvement.

## Recommendation

**Option B — the composable render-pass pipeline**, with a **pragmatic floor**:
simple single-line command output does not need a data model — its observer emits
one Catalog-keyed line directly (§5.4's inline-key `write!`). The pass pipeline is
for **composite, filterable** output (room `look`, character sheet, WHO), which is
exactly where the grim-map north-star and `grim-perception` need it. So the two
coexist: trivial output stays a direct `tr!` write; composite output builds a model
and runs passes.

**Where the pipeline lives:**

- The **pass registry, ordering, and the per-recipient render loop** live in
  **`grim-scene`** initially — it already owns the output path and the
  Pass/Buffer/Drop policy (§5.3), and rendering must respect that policy.
- **Each pass ships in the plugin that owns its data** (world → description/exits,
  actor → occupants, item → floor items), so no crate `use`s another's event type
  — §5.4 preserved.
- **Split later, not now.** If the render machinery grows enough to stand alone
  (or `grim-input` is carved out per ADR-0007), the pipeline moves to a dedicated
  `grim-render` crate. Per §3 ("split on the second implementation"), do **not**
  create `grim-render` speculatively — start in `grim-scene`.

## Interaction with other proposals

- **ADR-0004:** the observer that handles a command is what builds the data model
  and/or emits the direct line; render passes are the read-side of the same typed
  event.
- **ADR-0003:** the pipeline emits into a per-Session sink whose top Scene's
  output policy (Pass/Buffer/Drop) decides delivery.
- **`grim-perception` (§9):** the render loop is the seam where per-recipient
  filtering and `name_for` attach. This ADR does not build perception; it ensures
  the pipeline is per-recipient-shaped so perception is additive.

## Consequences

- `grim-scene`'s monolithic `format_output` is dissolved: composite output becomes
  data-model + registered passes; single-line output becomes a direct catalog
  write in the owning plugin.
- Authors can reorder and filter composite displays (the grim-map goal) by
  reordering/filtering passes, without editing engine crates.
- The output path becomes per-recipient-capable, unblocking `grim-perception`
  later without another rewrite.
- **Open questions for the maintainer:**
  - Is a **typed data model per composite command** (a `LookModel` struct) worth
    it, or should passes read the ECS directly at render time? Recommend a typed
    model for `look` (it is what makes the output testable and filterable), ECS-direct
    for trivial cases.
  - Confirm the pipeline **starts in `grim-scene`** rather than a new `grim-render`
    crate — the recommendation follows §3's start-simple rule, but this is the
    maintainer's crate-boundary call.
