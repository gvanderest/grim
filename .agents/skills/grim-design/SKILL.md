---
name: grim-design
description: Design a GRIM feature to the repo quality bar — arch review, ADR conflicts, retrofit triage, local iteration, issue publish. Use when designing a GRIM feature, planning an implementation, discussing architecture, or when the user says grim-design, design this, or requests a technical design.
---

# Grim Design

Default entry point for GRIM feature/design work in this repo. Wraps
`grilling` + `domain-modeling` (the `grill-with-docs` reasoning core) with the
repo-specific quality gates below. It does not replace generic design skills —
it adds the arch/standards + retrofit + publish steps they lack.

## Quick start

1. Resolve input: issue URL / `N` / free-text problem statement. No issue is
   valid — the tmp file is then the deliverable, with no publish step.
2. Preflight once: `gh auth status`. On failure, degrade to tmp-only and print
   the manual fallback commands.
3. Create `.planning/tmp/<slug>/DESIGN.md` (+ scratch alongside). Nothing from
   a design session touches tracked files until a later implementation branch.
4. Run the gates below, iterating locally until the user says `publish`,
   `post it`, or `comment it`.
5. On approval, continue via `grim-feature code` (re-enters at implementation
   with the packet on disk) — or `grim-code` directly for just the build.

## Gates

- **Arch review (mandatory).** Read in order: `docs/ARCHITECTURE.md` →
  `CONTEXT.md` vocabulary → scoped `docs/adr/*` → owning crate's `README.md` →
  `AGENTS.md`. Write a Constraints section citing each doc checked plus the
  binding constraints, or explicit "no constraint found in X".
- **Design reasoning.** Delegate to `grilling` + `domain-modeling` rounds.
  Default template: Problem / Constraints / Options / Decision / Standard
  set-or-followed / Retrofit scope / Tasks. ADR alternatives only on request.
- **ADR conflicts (scoped).** Review only ADRs touching the owning crate +
  direct dependents (plus `ARCHITECTURE.md` §8 if touched). Report conflicts
  vs the new design, vs each other, vs current code. Full-corpus sweeps are out.
- **New standards.** Draft in tmp as `ADR-DRAFT-<slug>.md` with `supersedes:` /
  `amends:` + rationale. `accepted` ADRs change only via superseding ADR, never
  in place; `Proposed` ADRs in scope may be amended. Never edit `docs/adr/` or
  its README index from design. Publish marks the draft `Proposed`; the
  implementing PR ratifies. Glossary/ADR side drafts stay in tmp the same way.
- **Retrofit triage.** Sweep owning crate + direct dependents (`lsp
  references`/`grep`), list every drift site, propose inline vs deferred per
  site with cost. User decides. Deferred = `gh issue create --title
  "[deferred] ..."`, `Deferred: #M` in the design, comment on `#N`.

## Publish

- Post the full design as an issue comment body. Gist (`gh gist create
  --secret`) only as overflow past ~60k chars, linked from the comment.
- Header embeds tmp path + date + `git rev-parse HEAD`. Post-publish iteration
  marks the published copy stale and offers re-publish (edit in place).
