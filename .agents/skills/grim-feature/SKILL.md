---
name: grim-feature
description: Run a GRIM feature end to end — grim-design, then grim-code, then grim-review-and-pr — carrying the packet between phases. Use for a fresh idea with no design yet, or when the user says grim-feature, take this feature from design to PR, or build it properly. Routing: packet on disk means grim-code; fresh idea means here.
---

# Grim Feature

Lifecycle runner for one feature: **design → code → review-and-pr →
babysit**. Each phase is its home skill; this skill owns the packet, the
handoffs, and the stop conditions. Nothing here duplicates phase internals —
it points at them.

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
4. **Babysit** (`grim-babysit-pr`). Automatic — never a stopping point.

### Auto-babysit rule

Opening the PR does not yield. The same run continues straight into
`grim-babysit-pr` with the review → done packet (PR URL/number + branch +
design path). Do not ask "want me to watch it?" — watching is the default.
The only exits from babysit are `MERGED` / `CLOSED`, user says stop, or a
conflict stop per the babysit skill.

### Merge watch (no token spin)

Do not poll in the agent turn loop. Park the wait in one supervised
background process and let it wake you:

- `hub op:start`, `name: "pr<N>-watch"`, polling every 60s while CI runs,
  backing off to 120–300s when quiet:
  `gh pr view <N> --json state,mergeStateStatus,reviewDecision,statusCheckRollup -q '...'`
  (or `gh pr checks --watch --interval 60` while checks are running).
- The watcher appends state changes to `.planning/tmp/<slug>/watch.log` and
  exits nonzero on `MERGED` / `CLOSED`; the agent `hub op:wait`s on it
  instead of spinning its own polls.
- Every push message states threads closed + CI state awaited (babysit rule).

### Post-merge issue verification

On `MERGED` (only then — never on green CI alone):

1. Re-read the originating issue body (`gh issue view <N> --json body`).
   Build a requirement checklist: every checkbox, every "should"/"must"
   sentence, every acceptance criterion.
2. Verify each item against the merged diff + CI evidence (`git diff
   main...<branch>`, test names/output). Mark each done / not-done with
   file + test pointers. Never mark from memory.
3. Ask the user one question with the table: close the issue
   (`gh issue close <N>`) or continue with the remaining items (new tasks
   re-enter at design with the same packet dir, new branch).
   Unchecked items never close silently.

## Resume and rerun

- Single phase by name: `grim-feature design|code|review` re-enters at that
  phase using the packet on disk. A missing packet is a hard stop, not a
  reconstruction exercise — ask.
- `grim-feature babysit <N>` (or with the PR URL) re-enters the watch loop
  for an already-open PR: derive branch + design path via `gh pr view`,
  verify branch, resume polling.
- Re-running code after a design amendment keeps the branch; re-running
  review after new commits re-assembles the packet from the tip.
- Kill conditions: user says stop; design unapproved; P0 unresolvable
  (file it, reference it, stop — do not route around).
