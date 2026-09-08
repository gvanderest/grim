---
name: grim-review-and-pr
description: Adversarial pre-merge review plus the commit-to-PR mechanics for GRIM — a reviewer subagent debates the diff for one round, findings graded fix-or-accept, then branch, commit, push, PR. Use when finishing a feature, before committing, before opening a PR, or when the user says grim-review-and-pr, review this diff, ship it, or cut a PR.
---

# Grim Review and PR

Pre-merge debate, not design review: design-phase critique belongs to
`grim-design`; this skill consumes its output (the tmp `DESIGN.md` or the
published issue comment) plus the diff. A reviewer subagent argues against the
diff; the implementer defends or fixes. Unresolved P0 blocks the merge. Then
the repo workflow (AGENTS.md): branch from `main`, incremental commits, push,
PR with an explicit body, CI, human review, squash merge.

## Quick start

1. Preflight: `git status` (clean except intended files), `git branch
   --show-current`, `gh auth status`. Wrong branch → branch from `main` first.
2. Assemble the packet: `git diff main...HEAD` (or `git diff` if uncommitted)
   plus the originating `.planning/tmp/<slug>/DESIGN.md` or the issue `#N`
   that ratified it.
3. Run the adversarial round below. Fix-or-accept every finding.
4. Commit → push → PR with an explicit `--body` (shape below). Pushing without
   a PR is unfinished. NEVER `--no-verify`. NOTE: bare `gh pr create --fill`
   leaves an empty body on single-line commits — always pass `--body`.

## Adversarial round

Spawn one `reviewer` subagent with the packet and these instructions: steer it
hostile to the diff, friendly to the architecture. One adversarial round: fixes
get a verification re-read against the finding, not a fresh round.

- Grade every finding: **P0** (wrong-crate placement, closed enum/type where an
  open registry belongs, format-once-broadcast, untranslatable player string,
  broken invariant, fail-open gate, stale crate README per rule 7a, coverage
  below the rule-10 floor) / **P1** (deferred: file `[deferred]…` issue,
  reference `Deferred: #M`) / **nit** (fix inline or drop).
- Argue each P0/P1 against `docs/ARCHITECTURE.md`, `CONTEXT.md`, scoped
  `docs/adr/*`, and the originating design. No grading without a cited
  constraint or an explicit "no constraint — judgment call".
- The implementer answers each finding: fix, or accept with a one-line reason.
  Accepted P0s must be re-argued, not silently kept. Any P0 without a fix or a
  reasoned acceptance blocks the PR. Example: finding "new `say` flag bypasses
  `ChannelMessage`" → accept "flag is transport framing, not audience; lives in
  telnet crate per §5.1" → PR body carries "Accepted P0: … (reason)".

## Commit and PR

- Commits are incremental and green: `make precommit` passes per commit
  (lint + coverage run at the hook; a red commit trains `--no-verify`).
- Push the branch; create the PR with `--body`, never bare `--fill`:
  design/issue link, accepted-P0 reasons, `Deferred: #M` references.
- CI (`make lint`, `make coverage`, `integration` copyover job)
  must be green before requesting human review.
