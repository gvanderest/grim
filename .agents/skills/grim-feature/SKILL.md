---
name: grim-feature
description: Run a GRIM feature end to end — grim-design, then grim-code, then grim-review-and-pr — carrying the packet between phases. Use for a fresh idea with no design yet, or when the user says grim-feature, take this feature from design to PR, or build it properly. Routing: packet on disk means grim-code; fresh idea means here.
---

# Grim Feature

Lifecycle runner for one feature: **design → code → review-and-pr**. Each
phase is its home skill; this skill owns the packet, the handoffs, and the
stop conditions. Nothing here duplicates phase internals — it points at them.

## The packet

One directory carries everything: `.planning/tmp/<slug>/` (design + drafts +
scratch) and one branch `<slug>` from `main`. Phase N's exit writes what phase
N+1's entry reads:

| Handoff | Contents |
|---|---|
| design → code | approved `DESIGN.md`: tasks, verdicts, `Deferred:` list |
| code → review | branch + design path + verification evidence + judgment-call log |
| review → done | PR URL + accepted-P0 reasons + `Deferred: #M` references |

Lose the packet and the next phase starts blank — never re-derive it.

## Run

1. **Design** (`grim-design`). Iterate locally until the user approves
   (verdicts recorded, drafts in tmp). No approval → stop. Nothing touches
   tracked files in this phase.
2. **Code** (`grim-code`). Branch from `main`, work the design's tasks,
   `make precommit` green throughout. Design drift goes back to phase 1 —
   the coder never silently re-scopes.
3. **Review-and-PR** (`grim-review-and-pr`). Adversarial round, fix-or-accept,
   `gh pr create --fill --base main --body "…"`, green CI before human review.

## Resume and rerun

- Single phase by name: `grim-feature design|code|review` re-enters at that
  phase using the packet on disk. A missing packet is a hard stop, not a
  reconstruction exercise — ask.
- Re-running code after a design amendment keeps the branch; re-running
  review after new commits re-assembles the packet from the tip.
- Kill conditions: user says stop; design unapproved; P0 unresolvable
  (file it, reference it, stop — do not route around).
