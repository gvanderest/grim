# Attempt/Fact typed events

status: Proposed

> ⚠️ **Needs maintainer decision — do not implement until accepted.** This ADR
> is a design proposal for one of the deferred §8 redesigns in
> [ARCHITECTURE.md](../ARCHITECTURE.md). It records a recommended direction for
> review; no code should change on its basis until it is marked `accepted`.

Today `SayEvent`/`MoveEvent` are **facts with no cancellable phase** — they fire
unconditionally, so nothing can veto a speech or a move (ARCHITECTURE.md §8, "No
attempt/fact split"). ARCHITECTURE.md §6 and CONTEXT.md already fix the target —
**events that can be denied come in pairs; events that already happened do not** —
and give the mechanism (`World::trigger_ref` for the immediate attempt phase, then
`trigger` for the fact). This ADR confirms the model, chooses how attempts are
represented, and settles how it interacts with the typed dispatch of ADR-0004.

## Terminology

Defined in [CONTEXT.md](../../CONTEXT.md): **Attempt** (a requested action
something may still deny or modify; imperative name — `Say`, `Move`, `Damage`),
**Fact** (an action that has happened; past-tense name — `Said`, `Moved`,
`Damaged`; carries final values), **Cancel reason** (why an Attempt was denied,
expressed as a **Key** + args). _Avoid_: Pre/Post — tense says which is
authoritative, ceremony does not (§6).

## Why a pair, and why only for attempts

§6 gives the one reason a pair exists: **Bevy guarantees nothing about the order
of observers watching the same event.** A mute plugin cannot ensure it runs before
the renderer, so a single event cannot both be vetoable and rendered
deterministically. The fix is a phase boundary — the attempt is resolved to
completion, *then* the fact fires. That reasoning applies **only to attempts**;
facts have nothing to veto, so pairing them "doubles the event surface for a
refusal nobody can cast" (§6).

## Options

### Option A — status quo: fact-only events

Keep `SayEvent`/`MoveEvent` as unconditional facts.

- **−** Nothing can veto or modify: no locked doors, no gag/mute, no drunk-garble,
  no immunity, no shield-reduces-damage. Every one of §6's motivating cases is
  impossible. Listed only to reject.

### Option B — one event with a `cancelled` field, single phase

A single `Say { actor, text, cancelled: Option<CancelReason> }` triggered once;
observers set `cancelled`; the same observers also render.

- **−** Reintroduces the ordering bug §6 names: the renderer observer might run
  before the mute observer sets `cancelled`. There is no phase boundary, so
  "arbitrary observer order" decides whether a vetoed message is still printed.
  Rejected by §6's core argument.

### Option C — attempt/fact pair over one sync point (RECOMMENDED)

An **attempt** struct with a monotonic `cancelled: Option<CancelReason>` latch is
`trigger_ref`'d (runs every vetoer *immediately*); if still uncancelled, the
**fact** is triggered carrying the possibly-modified final values. Both share one
sync point via a queued `World` closure (§6):

```rust
commands.queue(move |world: &mut World| {
    let mut attempt = Say { actor, text, cancelled: None };
    world.trigger_ref(&mut attempt);              // every vetoer runs now
    if attempt.cancelled.is_none() {
        world.trigger(Said { actor, text: attempt.text });  // possibly rewritten
    }
});
```

- **+** Arbitrary observer order stops mattering — cancellation is a monotonic
  latch, so order cannot change the outcome (§6).
- **+** A pair costs no extra latency: one sync point, not one per phase (§6).
- **+** Attempts are **mutable, not merely vetoable** — drunk garbles text, a
  shield reduces a number; the fact carries the final values (§6).
- **+** `CancelReason` is a plain `(key, default, args)` struct (§6), so refusals
  are Catalog-overridable, extraction-visible, `Clone`, and serializable — the
  same text mechanism as everything else (§5.4), not a second one.
- **−** Two types per denyable action instead of one, and a `commands.queue`
  closure at each call site. Accepted: it is the price of deterministic veto.

## Recommendation

**Option C — the attempt/fact pair over a single `trigger_ref`+`trigger` sync
point**, exactly as §6 specifies. It is the only option that makes veto
deterministic without a latency cost, and it makes attempts modifiable (not just
blockable), which the drunk/shield cases require. `CancelReason` stays the plain
`(key, default, args)` struct; **delivery of the refusal belongs to the
dispatcher, not the vetoer** (§6), so refusal formatting stays in one place.

**Only denyable actions get a pair.** Facts that nobody can veto
(`LoggedIn`, `ConnectionClosed`, `Said` once it has fired) stay single events.
Naming is by tense — imperative attempt, past-tense fact.

Accepted known nondeterminism (from §6): if two systems veto the same attempt,
observer order decides *which* reason the player sees. The outcome is deterministic
(cancelled either way) and both reasons are true, so ranking reasons is ceremony
for a case players cannot detect.

## Interaction with ADR-0004 (typed dispatch)

The two are complementary and share the observer mechanism:

- A command's observer (ADR-0004) is the caller that **raises the attempt**. `look`
  produces no attempt (it denies nothing); `say`/`move`/`attack` do.
- Because ADR-0004 dispatches via `trigger`, the attempt/fact pair slots in
  directly — both are triggered events, and `trigger_ref` (immediate) is the piece
  ADR-0004's Option C (bare handlers) could not offer. This is a concrete reason
  ADR-0004 keeps a typed event surface rather than running logic in a bare closure.
- The fact event (`Said`, `Moved`) is what the render pipeline (ADR-0005) and any
  logging/moderation observe. The attempt is internal to the deny/modify phase.

## Consequences

- `SayEvent`/`MoveEvent` split into `Say`/`Said`, `Move`/`Moved`; each denyable
  action gains an imperative attempt + a past-tense fact.
- Gag/mute, locked doors, immunity, drunk-garble, and shield-reduction become
  expressible as attempt observers — none was possible before.
- Refusals are author-overridable Catalog text, serializable for moderation logs
  and testable in E2E scenarios (§10).
- `data: Option<Box<dyn Any + …>>` on `CancelReason` is a **future field, not part
  of this change** — added only if game logic ever needs to *react* to a cause
  (a mob unlocking the door that blocked it) rather than print it (§6).
- **Open question for the maintainer:** confirm the **initial set** of denyable
  actions to convert — the recommendation converts `Say` and `Move` first (they
  exist as facts today) and leaves `Damage`/combat to arrive with `grim-combat`
  (ADR-0007 / §9), rather than converting speculatively.
