---
name: grim-code
description: Implement an approved GRIM design to the repo quality bar — branch, build per the design's tasks, keep every commit green. Use after a grim-design completes (approved DESIGN.md on disk), or when the user says grim-code or implement this design. Routing: packet on disk means code; a fresh idea with no design means grim-feature.
---

# Grim Code

Post-design implementation. Consumes an approved design; produces a green
branch. It does NOT open a PR — that belongs to `grim-review-and-pr`.

## Quick start

1. Preflight: `git status` clean, `git branch --show-current`. Take the packet:
   `.planning/tmp/<slug>/DESIGN.md` (tasks + verdicts + `Deferred:` list) or the
   ratifying issue `#N`. No packet → stop and ask; never invent scope.
2. `git fetch origin && git checkout -b <slug> origin/main`. Branch name matches the design slug. Always base on the latest `origin/main` — a stale local `main` silently forks behind landed work — unless the design or the user names another base explicitly.
3. Work the design's Tasks in order, one slice per commit. Between slices,
   re-read the design — it is the spec; drift from it is a design change that
   goes back through `grim-design`, not a judgment call at the keyboard.
4. Hand off: branch name + design path + verification evidence + judgment-call
   log (open questions, deviations, risky calls the reviewer should probe).
   The reviewer needs all four.

## Discipline (AGENTS.md, enforced not suggested)

- Re-ground after every edit; read the whole function before touching it.
- Fix the source, never suppress the symptom; no shims, aliases, or
  deprecated paths — clean cutover, every caller migrated.
- Crate README updated in the same change (rule 7a), especially the
  Commands→handler table. Coverage: rule-10 floor holds per commit.
- New subsystem behavior gets an E2E scenario (ARCHITECTURE.md §10), not only
  unit tests. Retrofit sites marked inline-vs-deferred in the design stay that
  way; new drift found mid-flight is filed as a `[deferred]…` issue, noted as
  `Deferred: #M` in the design, and commented on the originating issue —
  never silently scoped in.
- `make precommit` green per commit (lint, fmt, coverage run at the hook; a
  red commit trains `--no-verify`).

## Verify before handoff

- Deliverable proof per change type: bug fix reproduces-then-passes; feature
  exercises the changed path. Tests earn their place per the Verify rules —
  throwaway scripts prove work, kept tests guard contracts.
- Handoff summary: what changed per design task, what was deferred (`#M`),
  and anything the design got wrong (feeds back into it).
