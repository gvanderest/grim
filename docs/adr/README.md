# Architecture Decision Records

Each ADR records one architectural decision: the context, the options weighed, and
the choice with its rationale. [ARCHITECTURE.md](../ARCHITECTURE.md) is
authoritative for the target architecture; [CONTEXT.md](../../CONTEXT.md) fixes the
vocabulary. An ADR is `accepted` once its direction is settled and `Proposed` while
it awaits a maintainer decision.

## Index

| # | Title | Status |
|---|-------|--------|
| [0001](0001-area-identity-and-instancing.md) | Area identity, addressing, and instancing | accepted |
| [0002](0002-character-class-tiers.md) | Character gender, race, and class tiers | accepted |
| [0003](0003-scene-stack-entity-model.md) | Scene-stack entity model | **Proposed** |
| [0004](0004-typed-per-command-dispatch.md) | Typed per-command dispatch | **Proposed** |
| [0005](0005-per-plugin-render-pipeline.md) | Per-plugin render pipeline | **Proposed** |
| [0006](0006-attempt-fact-typed-events.md) | Attempt/Fact typed events | **Proposed** |
| [0007](0007-north-star-layers.md) | New layers implied by the north-star scenarios | **Proposed** |

## Proposed — needs maintainer decision

ADRs 0003–0007 are **design proposals for the deferred §8 redesigns** in
ARCHITECTURE.md. They are decision-ready but **not accepted** — do not implement
against a `Proposed` ADR until the maintainer marks it `accepted`. Each covers one
fork left open after the crate-layering work:

- **0003 — Scene-stack entity model.** Retire the closed `ClientState` enum for a
  pushable stack of Scene entities (§5.3). *Recommends:* a stack of Scene entities
  with `EntityEvent` input routing, living in `grim-scene`.
- **0004 — Typed per-command dispatch.** Retire the closed `Command` enum and the
  N-systems-filter-one-event pattern (§5.2). *Recommends:* per-command events
  dispatched via observers, routed by the `grim-command` registry.
- **0005 — Per-plugin render pipeline.** Dissolve the central
  `grim-scene::format_output` (§5.4). *Recommends:* a composable
  data-model → ordered-render-pass pipeline (grim-map north-star), starting in
  `grim-scene`.
- **0006 — Attempt/Fact typed events.** Add vetoable/modifiable attempts paired
  with facts (§6). *Recommends:* the attempt/fact pair over one
  `trigger_ref`+`trigger` sync point, with `CancelReason` as Catalog-keyed data.
- **0007 — North-star layers.** Sketch the boundaries and dependency directions
  for `grim-item`, `grim-skills`, the player config registry, the container
  open/close pattern, and (speculative) `grim-input`. *Recommends:* `grim-item` +
  config registry soon, `grim-skills` on its second consumer, `grim-input` not yet.

Reading order for the session/dispatch cluster: **0003 → 0004 → 0006 → 0005**, then
**0007** for the layers they enable. 0003 and 0004 are coupled (the closed-enum
retirements share the session rework, per §8), and 0006 rides 0004's typed-event
surface.
