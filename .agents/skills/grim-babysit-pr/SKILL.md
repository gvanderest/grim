---
name: grim-babysit-pr
description: Watch an open GRIM pull request until it lands — poll CI and review threads, implement feedback, commit and push per group, escalate conflicts. Use when a PR already exists and the user says grim-babysit-pr, watch PR N, or babysit this PR. Routing: PR open means babysit; no PR yet means grim-review-and-pr.
---

# Grim Babysit PR

Owns a PR from open to merged. Thread-fix mechanics belong to the `fix` skill
(unanswered-thread query, one commit per group, comment + resolve); this skill
owns the watch loop and the one rule that matters: **feedback conflicting with
the design or the standards stops for user input — never assumptions.**

## Quick start

1. Take the packet: PR number `#N` (from the review→done handoff's PR URL or
   the user). Derive the rest via `gh`: branch from `headRefName`, design path
   from the PR body links, CI state from `gh pr checks`. For review threads,
   use the `fix` skill's unanswered-thread query — the plain
   reviews/comments fields cannot resolve it.
2. Verify branch (`git branch --show-current` matches `headRefName`) before
   every commit. Loop until `state` is `MERGED`/`CLOSED` or the user says stop:
   - CI red → read the failing job log, fix the source (never the gate),
     confirm `make precommit` green, commit, push, re-poll.
   - New review threads → hand the unanswered set to the `fix` skill flow
     (green per commit, grouped pushes), then re-poll. Bot threads get the
     same flow as human ones — triage, fix-or-accept with a reason, reply,
     resolve.
   - Checks pending (CI jobs or bot reviewers such as CodeRabbit) → keep
     polling; **pending is not passing**. NEVER declare a check "not a gate"
     without evidence: consult the repo's required-checks list or a completed
     pass, never the check's name. A `rate limited` / skipped bot pass after
     its threads were triaged counts as complete; an unreviewed push does not.
   - Approvals with no threads → report and keep polling for the human
     squash-merge; do not merge (merging is the human's call per AGENTS.md).
     Green CI with checks still pending is not done — the loop exits only on
     `MERGED` / `CLOSED`, the user saying stop, or a conflict stop below.
3. Poll concretely: `gh pr checks --watch --interval 60` while CI runs;
   thread polls every 120–300s with backoff when quiet. Never spin.
   Announce each push with what changed and what is still pending.

## Conflict rule (the load-bearing part)

Feedback contradicting any of these stops the loop for user input: the design
(tasks, verdicts, `Deferred:` list, judgment-call log, accepted-P0 reasons),
`docs/ARCHITECTURE.md`, `CONTEXT.md`, a `docs/adr/*` status, the owning crate's
README, or `AGENTS.md`. Present the concrete options — amend the design (back
through `grim-design`), overrule the reviewer with a reasoned PR reply, or
accept the feedback and record the standard change — and execute none without
an answer. Silence is also a stop; never time out into an assumption.

Example: reviewer asks for a central formatter addition → conflicts with §5.4
(formatting lives in the owning plugin) → stop, ask, do not "just add it".

## Exit

- `MERGED` → report the squash commit, close the loop, archive nothing (the
  PR is the record).
- `CLOSED` unmerged → report why, keep the branch for salvage.
- Every push message states the threads closed and the CI state awaited.
