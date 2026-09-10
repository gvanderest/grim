---
name: grim-review-and-pr
description: Adversarial pre-merge review plus the push-to-PR mechanics for GRIM — a reviewer subagent debates the diff for one round, findings graded fix-or-accept, then PR with an explicit body. Use before committing, before opening a PR, or when the user says grim-review-and-pr, review this diff, ship it, or cut a PR. Branch only if needed; grim-code owns branching.
---

# Grim Review and PR

Pre-merge debate, not design review: design-phase critique belongs to
`grim-design`; this skill consumes its output (the tmp `DESIGN.md` or the
published issue comment) plus the diff. A reviewer subagent argues against the
diff; the implementer defends or fixes. Unresolved P0 blocks the merge. Then
the repo workflow (AGENTS.md): push, PR with design link + accepted-P0 reasons
+ `Deferred:` references, CI, human review, squash merge.

## Quick start

1. Preflight: `git status` (clean except intended files), `git branch
   --show-current`, `gh auth status`. On the wrong branch, switch to the
   packet branch from `grim-code`; branch fresh from `main` only when no
   packet exists.
2. Assemble the packet: `git diff main...HEAD` plus `git diff HEAD` (staged and
   unstaged tracked changes) and the contents of every intended untracked file
   — bare `git diff` omits staged changes and untracked files — plus the
   originating `.planning/tmp/<slug>/DESIGN.md` or the issue `#N` that
   ratified it, and the handoff's verification evidence — spot-check one
   evidence claim yourself before the reviewer grades.
3. Run the adversarial round below. Fix-or-accept every finding.
4. Commit remaining fixes → push → `gh pr create --fill --base main --body
   "…"` (AGENTS.md command plus an explicit body: bare `--fill` leaves it
   empty on single-line commits). Pushing without a PR is unfinished. NEVER
   `--no-verify`.

## Adversarial round

Spawn one `reviewer` subagent with the packet and these instructions: steer it
hostile to the diff, friendly to the architecture. One adversarial round: fixes
get a verification re-read against the finding, not a fresh round.

- Grade every finding: **P0** (wrong-crate placement, closed enum/type where an
  open registry belongs, format-once-broadcast, untranslatable player string,
  broken invariant, fail-open gate, stale crate README per rule 7a, coverage
  below the rule-10 floor) / **P1** (deferred: file `[deferred]…` issue, note
  `Deferred: #M` in the design, comment on the originating issue) /
  **nit** (fix inline or drop).
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
  (lint, fmt, coverage run at the hook; a red commit trains `--no-verify`).
- Push the branch; create the PR per step 4: design/issue link, accepted-P0
  reasons, `Deferred: #M` references.
- CI (build, lint, test per AGENTS.md — the `check` job plus the `integration`
  copyover job) must be green before requesting human review. After the PR is
  open, continue via `grim-babysit-pr` — it owns the PR to merge. Do not stop
  at green CI with checks still pending or threads untriaged: that is the
  babysit loop's work, not a handoff point.
