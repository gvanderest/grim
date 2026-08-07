# Typed per-command dispatch

status: Proposed

> ⚠️ **Needs maintainer decision — do not implement until accepted.** This ADR
> is a design proposal for one of the deferred §8 redesigns in
> [ARCHITECTURE.md](../ARCHITECTURE.md). It records a recommended direction for
> review; no code should change on its basis until it is marked `accepted`.

Command dispatch still runs through a **closed `Command` enum**: input resolves to
one `Command` value, and the world/channel/scene plugins each read that single
event and `match` on it (ARCHITECTURE.md §8, and the "gaps" table row "`Command`
is a closed enum"). ARCHITECTURE.md §5.2 already fixes the target — a plugin
defines **its own event type** and registers a factory via `add_command` — and a
spike (`crates/grim/examples/spike_handler.rs`) proved the erased-handler
signature compiles. This ADR chooses the dispatch mechanism that replaces the
enum, and settles the polling-vs-routing trade-off §5.2 raises.

## Terminology

Defined in [CONTEXT.md](../../CONTEXT.md): **Command** (one line-of-text intent),
**Router** (resolves the first word to the plugin that registered it), **Actor**
(the entity a Command is attributed to). This ADR uses **event** for a Bevy
`Event`/`Message` type and **observer** for a Bevy observer watching a triggered
event.

## Problem with the closed enum

- **Unextendable.** A downstream author cannot add a variant to `Command` in a
  crate they do not own (§5.2). Every new verb is an engine edit.
- **Fan-out cost scales in plugin count.** A shared command event means every
  plugin runs a reader every frame and discards almost everything; cost scales
  with installed plugins, not with the command actually issued (§5.2).
- **`match` concentration.** Adding a verb touches the enum *and* every `match`
  that must stay exhaustive — the same concentration ADR-0003 removes from
  `ClientState`.

The `commands/<name>.rs` files already isolate one command each. The upgrade
should **not move those files** — it should change how each one is *dispatched*,
turning a `match` arm into a registered typed handler in place.

## Options

### Option A — typed per-command `Message` (buffered events)

Each command is its own `#[derive(Message)]` type; `add_command` wires the factory
to *write* that message; each plugin adds a system reading its own message type.

- **+** Batch-friendly: a frame's worth of the same command drains in one read;
  natural fit for tick-based systems.
- **+** Typed and open — the enum is gone.
- **−** Dispatch is O(readers) per type: every registered command type adds a
  system that runs every frame to check its own buffer, even when empty. At
  hundreds of commands this is hundreds of empty drains per frame — a milder
  version of the very fan-out §5.2 rejects.
- **−** Deferred by a frame: the message is read on the *next* system run, so a
  command → effect → render chain costs frame hops (the "single `.after()` chain"
  hazard in §8).

### Option B — Bevy observers via `trigger` (RECOMMENDED)

Each command is its own `#[derive(Event)]` type; `add_command` registers an erased
factory (the spike's `Handler` boxed closure) that `trigger`s the typed event;
each plugin `add_observer`s its handler. Dispatch fires **exactly one** typed
event, so **only** the observer watching that type runs — flat in installed-plugin
count (§5.2's stated goal).

- **+** Direct, synchronous dispatch: one trie walk, one boxed call, one observer
  (§5.2). No empty per-frame drains.
- **+** Flat cost in plugin count — the property §5.2 explicitly wants.
- **+** Composes with the attempt/fact sync point (ADR-0006): attempts are
  `trigger_ref`'d immediately, which only observers support.
- **+** Composes with ADR-0003: Scene input routing is already `EntityEvent`
  observer dispatch, so session and command dispatch share one mechanism.
- **−** Unbatched: N identical commands in a frame fire N triggers. At MUD input
  rates (one line per player per few hundred ms) this is irrelevant.
- **−** Observers run synchronously inside the triggering system's command queue;
  a handler that wants to defer heavy work still queues it — no free batching.

### Option C — command registry with boxed handlers (no per-command event type)

Keep `add_command`'s registry, but the boxed handler *is* the command logic —
`Fn(&mut Commands, Entity, &str)` runs the effect directly, no event trigger.

- **+** Simplest dispatch: resolve → call. No event type per command at all.
- **−** No event surface means **nothing else can observe a command**. Attempts
  (ADR-0006), logging, and moderation all want to see "a Say was attempted" as an
  event; a bare handler hides it. §5.2's design deliberately keeps a typed event
  *because* other systems watch it.
- **−** Pushes all command logic into the closure or a function it calls, rather
  than into an observer that other plugins can also watch — weaker composition.

## Recommendation

**Option B — per-command events routed by the `grim-command` registry and
dispatched via observers.** It is the only option that delivers the flat-cost
dispatch §5.2 names as the goal *and* leaves a typed event surface for ADR-0006's
attempt/fact split and for logging/moderation. Option A reintroduces per-frame
fan-out and frame-hop latency; Option C saves an event type but blinds every other
subsystem to what commands happened.

`add_command` stays generic over `E: Event` with the spike's bound
(`for<'a> E::Trigger<'a>: Default`), erasing the concrete type into the uniform
boxed `Handler`. One registry holds any number of unrelated command types (§5.2).

**Where the polling-vs-routing line falls:** routing (observers) for command
*dispatch*; polling (buffered `Message`/systems) stays the right tool for
*tick-driven* subsystems that genuinely batch — regen, respawn, combat rounds.
This ADR does not push those toward observers; it only removes the shared
`Command` enum from the input path.

## Implications

- **`commands/<name>.rs` files stay put.** Each stops being a `match` arm and
  becomes: a command event type + a factory registration + an observer. The file
  boundary the code already has is the right one; only the wiring inside changes.
- **`grim-command` owns the registry** (it already holds `CommandRegistry<C>` and
  is transport-blind, §5.2). It becomes generic over the *erased* handler rather
  than a concrete `Command`, so it carries no game vocabulary at all — completing
  the direction §8 records (the registry already no longer *depends* on the enum;
  grim-scene/world/channel still `match` it).
- **The closed `Command` enum is deleted** once every `match` site is a registered
  observer. This is coupled to ADR-0003 because a few sites (who/where/quit) need
  Session context an observer must be given explicitly — the scene rewrite is what
  supplies it (§8 notes this coupling).
- **Channels-as-data (§7)** ride the same change: `say`/`yell`/`ooc` stop being
  three coded handlers and become `add_channel` data registrations producing one
  scope-resolved `ChannelMessage` — that event legitimately has one observer (§7),
  so it is *not* the fan-out §5.2 rejects.

## Consequences

- Dispatch cost becomes flat in installed-plugin count; a shadowed prefix is still
  surfaced by `contested_prefixes()` at startup (already built, §8 step 3).
- Third parties add verbs by adding a type + observer, never by editing an engine
  crate.
- Unit tests that `match` on `Command` are rewritten to assert an observer fired /
  a fact event was emitted; E2E scenarios (§10) that drive `say hello` and check
  output are unaffected.
- **Open question for the maintainer:** confirm the tick-driven subsystems
  (combat rounds, regen) are explicitly *out* of this change and keep buffered
  `Message` dispatch — the recommendation assumes routing for input, polling for
  ticks, not routing everywhere.
