# GRIM Architecture

GRIM is a foundation for building MUDs. It is not a MUD.

Everything GRIM does is registered rather than hardcoded, and anything registered can
be replaced. The binary is not part of GRIM — it belongs to the person building a MUD,
and its job is to compose plugins.

See [CONTEXT.md](../CONTEXT.md) for the vocabulary used throughout this document.
Terms defined there (Connection, Session, Scene, Command, Catalog, …) are used here
with exactly those meanings.

---

## 1. The binary composes; the libraries provide

`crates/example-mud` is the only binary in this workspace. Everything else is a
library. A MUD author's own binary looks the same.

```rust
fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    // Foundation
    app.add_plugins(GrimTextPlugin);
    app.add_plugins(GrimCommandPlugin);
    app.add_plugins(GrimNetworkingPlugin);
    app.add_plugins(GrimScenePlugin);

    // Transports registered against networking
    app.add_plugins(TelnetPlugin::new(4000));

    // Gameplay
    app.add_plugins(GrimWorldPlugin);
    app.add_plugins(GrimChannelPlugin);
    app.add_plugins(GrimAuthPlugin);
    app.add_plugins(GrimPersistencePlugin);

    app.add_systems(Startup, seed::seed_world);
    app.run();
}
```

Nothing in this file is privileged. Every plugin listed is one a MUD author could
omit, replace, or precede with their own.

The `grim` crate exists so an author who wants none of these choices can depend on one
crate, take the defaults, and only think about their own assets and data.

---

## 2. Capabilities are extension points — not every crate is a plugin

The rule is **every capability is an extension point**, not "every crate is a plugin."

A `Plugin` impl exists to put something into the `App`: a system, a resource, an
observer, an event. A crate with none of those gains nothing from one. An empty
`impl Plugin` is worse than no plugin, because it becomes a registration the author
must remember or the crate silently does nothing.

| Kind | Examples | Why |
|------|----------|-----|
| Plugin | `grim-networking`, `grim-command`, `grim-scene`, `grim-text`, `grim-world` | own resources, systems, observers |
| Plain library | `grim-color`, validation helpers | pure functions, no `App` state |

`grim-color` is pure functions over strings and has no Bevy dependency at all. That is
deliberate: it is the one piece testable without an `App`.

---

## 3. Crate naming

**`grim-<system>`** — a subsystem. One plugin per crate.

**`grim-<system>-<extension>`** — an implementation registered against that subsystem,
where alternatives are mutually exclusive. The name states what it plugs into.

There is no `grim-core-*`. The `grim-` prefix already marks a crate as ours, so `core`
carries no information and would force an argument about what qualifies at every new
crate.

### Start simple, split on the second implementation

A new system starts as a single `grim-<system>` crate. It splits into
`grim-<system>` (contract and shapes) plus `grim-<system>-<extension>` crates only
when a **second real implementation** appears.

Speculative extension points are the expensive kind of wrong: the contract gets
designed against one imagined consumer and fits none.

**`grim-networking` is already past that threshold** — telnet, SSH, and WebSocket are
three real transports, so it starts split. Everything else starts as one crate.

### One plugin per crate, but many registrations

"One plugin per crate" constrains *plugins*, not *registrations*. `grim-channel` is one
plugin registering many channels, each of which registers a command. `grim-auth` is one
plugin registering five scenes. That is correct and not a violation.

---

## 4. Crate map

| Crate | Plugin | Owns |
|-------|--------|------|
| `grim-color` | — | colour codes, ANSI rendering, palette |
| `grim-text` | `GrimTextPlugin` | the Catalog: strings, templates, interpolation |
| `grim-command` | `GrimCommandPlugin` | command registry, resolution, dispatch |
| `grim-networking` | `GrimNetworkingPlugin` | tokio bridge, `Connection`, `Transport` trait |
| `grim-networking-telnet` | `TelnetPlugin` | telnet `Transport`, IAC negotiation |
| `grim-networking-ssh` | `SshPlugin` | SSH `Transport` |
| `grim-networking-websocket` | `WebsocketPlugin` | WebSocket `Transport` |
| `grim-scene` | `GrimScenePlugin` | the Scene stack, input routing, output policy |
| `grim-auth` | `GrimAuthPlugin` | login, account/character creation, selection, MOTD scenes |
| `grim-editor` | `GrimEditorPlugin` | the Editor scene |
| `grim-world` | `GrimWorldPlugin` | rooms, areas, exits, room-address lookups (being-free) |
| `grim-actor` | `ActorPlugin` | the beings (`Character`/`Player`/`InRoom`/…) + the being-reading verbs (`look`/`move`/`goto`/`quit`/`title`/`shutdown`) |
| `grim-channel` | `GrimChannelPlugin` | channel registry, audience, eligibility (§7) |
| `grim-persistence` | `GrimPersistencePlugin` | account/character save and load, player aliases, channel toggles |
| `grim` | — | facade: re-exports and a default plugin group |
| `example-mud` | *binary* | composition and world seed |

Dependency direction:

```
grim-color ──────┐
                 ├──> grim-networking-telnet ──┐
                 │                             ├──> grim-networking
grim-text ───────┤                             │
                 ├──> grim-scene <─────────────┘
                 │        │
                 │        v
                 └──> grim-command <──── grim-world, grim-channel, grim-auth, grim-editor
```

`grim-command` depends on nothing but Bevy.

`grim-actor` (the beings + being-reading verbs) sits **above** `grim-world`:
`grim-actor ──> grim-world`, never the reverse. `grim-world` is being-free — it
owns the room topology, not who stands in it — so the actor layer depends on it
while `grim-channel`, `grim-persistence`, `grim-networking-telnet`, and
`grim-scene` in turn depend on `grim-actor` for the being types.

---

## 5. The four dispatch mechanisms

Everything a MUD author registers goes through one of these four.

### 5.1 Transport — bytes on a wire

`grim-networking` owns the tokio bridge **once**: the runtime thread, the channels,
`Connection` spawn and despawn, and the `ConnectionInput` / `ConnectionOutput`
messages. A transport crate implements `Transport` — bind, accept, frame bytes into
lines, and lines back into bytes — and inherits the bridge.

Bevy's schedule is synchronous and tokio wants to own its threads, so a channel seam
between them is the normal answer. The alternative, running async TCP on Bevy's
executor, means reimplementing what SSH's crypto stack needs anyway.

**Telnet and SSH are special.** They are thin enough that GRIM must also act as the
user's terminal: ANSI escape sequences, echo suppression for passwords, line endings.
That work is *rendering*, and telnet and SSH share it — so it lives in `grim-color`,
which both depend on. WebSocket assumes a browser client that styles text itself, so
it simply does not depend on `grim-color`. There is no `PassthroughProtocol`, because
passthrough is the absence of a dependency rather than a class.

Note that transports share *rendering*, not *transport*: SSH's crypto handshake has
nothing in common with telnet's raw accept. That is why the seam is `Transport` plus
`grim-color`, and not one `Protocol` trait covering both axes.

### 5.2 Command — a line of input becomes an intent

A plugin defines **its own event type** and registers a factory:

```rust
#[derive(Event)]
pub struct Look { pub actor: Entity, pub target: Option<String> }

app.add_command("look", |actor, rest| Look {
    actor,
    target: (!rest.is_empty()).then(|| rest.to_string()),
});
app.add_observer(on_look);
```

`add_command` is generic over `E: Event`. The concrete type is erased at the
registration site into a uniform boxed closure, so one registry holds any number of
unrelated command types. The erased type is

```rust
type Handler = Box<dyn for<'w, 's> Fn(&mut Commands<'w, 's>, Entity, &str) + Send + Sync>;
```

The higher-ranked bound over `Commands<'w, 's>` is required and does work — verified
by a spike (`crates/grim/examples/spike_handler.rs`). Two Bevy-0.19 details the
signature must carry: `Event` has an associated `Trigger<'a>` type, and
`Commands::trigger` requires `for<'a> E::Trigger<'a>: Default`, so `add_command`'s
bound is `E: Event, for<'a> E::Trigger<'a>: Default`. A plain `#[derive(Event)]`
satisfies it (its global trigger is `Default`), but the bound must be written or the
erasure will not compile.

**There is deliberately no generic `MudCommand`.** A shared command event would mean
every plugin runs a reader every frame, discarding almost everything, with cost
scaling in plugin count. Instead, dispatch triggers exactly one typed event, so only
the observers watching that specific type run. The cost of resolving a command is one
trie walk, one boxed call, and one observer — **flat in the number of installed
plugins**.

A closed `Command` enum would also be unextendable: a downstream author cannot add a
variant to a crate they do not own. Under this design they never need to; they add a
type.

**`grim-command` is transport-blind.** Its input is "this entity submitted this line."
A mob's AI, an admin console, or a test can submit `"kill goblin"` with no
`Connection` in existence. That is the point — not purity, but that none of those
callers has to fake a socket.

#### Resolution order

```
raw line → ! repeat → player alias (single pass) → exact match → prefix by priority
```

- **Player aliases** are per-account state owned by `grim-persistence`, expanded
  **exactly once**. Single-pass expansion makes recursion structurally impossible —
  including mutual (`a`→`b`, `b`→`a`) and long chains — so no cycle detection is
  needed.
- **No semicolon splitting.** One line in, one command. Batching belongs to the
  player's terminal client.
- **Prefix matching resolves by priority**, defaulting to registration order. Players
  expect `n` to move north without a disambiguation prompt, so resolution always
  produces an answer.
- **Priority is an ordered list, not a number.** Commands live in one ordered
  collection and a prefix resolves to the earliest match in it. `prioritize_command`
  moves a command to the top, `deprioritize_command` to the bottom. There are no
  priority values to compare, allocate, or leave gaps in — the order *is* the
  ordering, so an author never has to reason about what number beats what.
- **Contested prefixes are reported at startup**, silent when uncontested:

  ```
  grim-command: 3 contested prefixes
    n → north  (also: nuke, note)
  ```

  Without this the author learns about a collision from a player's bug report. The
  report is what makes `nudge` usable.

### 5.3 Scene — what a line of input *means*

A **Scene stack** replaces a single state field. Input is always interpreted by the
topmost Scene; Scenes are pushed and popped, not switched.

This is what makes "in the Editor while still standing in the world" representable. A
single state value cannot express it — the Playing state would have to be smuggled
inside the Editor variant.

There is no separate "Modal" concept. A layered Scene is not a different kind of
thing, and a second word for it would only raise the question of which to use.

A Scene is an **entity** carrying its own marker component and data, so third parties
can define Scenes. Input is delivered as an `EntityEvent` targeted at the top layer's
entity, so only that layer's observer runs — the same flat-cost dispatch as commands.

Each Scene declares an **output policy** for game output arriving while it is on top:

| Policy | Behaviour |
|--------|-----------|
| Pass | default — output reaches the player |
| Buffer | held, flushed when the Scene is popped |
| Drop | discarded |

The Editor buffers, so a room conversation cannot splatter into prose a player is
composing.

`grim-scene` is also the **bridge** between networking and commands: it reads
`ConnectionInput` and decides, per Scene, whether the line is a password, a menu
choice, or a Command. This is why `grim-command` never needs to know about transports.

### 5.4 Catalog — every piece of author-facing text

**Formatting lives in the plugin that owns the event.** `grim-world` owns both `Look`
and the observer that renders it. A central formatter cannot survive the plugin model:
it would have to `use` every event type in the engine, so every third-party command
would require editing a crate the author does not own.

**Authors override the string, not the code.** That is what makes the Catalog a seam
rather than a constant.

```
strings/<locale>/*.json        single-line entries, merged
templates/<locale>/<key>.txt   multi-line layout blocks, one file per key
```

Both load into **one namespace**. `text("room.display")` does not know which
directory an entry came from. Single-line versus multi-line is a filing convention, so
putting a key in the "wrong" place is untidy but never a lookup failure. Two lookup
paths would recreate exactly the boundary problem the split is meant to avoid.

Two storage formats exist because neither alone is bearable: multi-line content in
JSON is `"You see:\n  {Wexits{x: %{exits}\n"`, and 400 one-liners as 400 files is no
better.

**Merge order is deterministic:** plugin registration order, then the author's own
directory last. Two plugins defining the same key must not resolve by filesystem
iteration order, and the author must be able to override anything.

**Inline defaults with a key.** A coder writes the English beside the code and gets an
override point for free:

```rust
write!(actor, "You %{noun} the %{target} for %{damage} damage!",
       key = "combat.damage.hit", noun = "hit", target = name, damage = 123);
```

The Catalog wins when the key is present; the inline string is the fallback. This is
gettext's model with a stable key instead of the source string.

**This needs an extraction tool** (`cargo grim strings extract`) or authors can never
discover what is overridable. Extraction should store a hash of each inline default,
so editing the English without bumping the key flags translations as stale — gettext's
"fuzzy". Without that, explicit keys silently drift from their translations, which is
the one place this model is weaker than gettext's.

**One macro, GRIM's own.** GRIM provides the `tr!`-style macro and it is used
universally. Colour codes live inside catalog strings (`{MYou say {x'{m%{text}{x'`),
and a general i18n library's interpolation collides with that brace syntax.

#### Perspective

One action produces different text per viewer. Roles are named, not numbered:

| Key | Viewer |
|-----|--------|
| `combat.damage.hit.actor` | the one acting |
| `combat.damage.hit.target` | the one acted upon |
| `combat.damage.hit.observer` | everyone else in the room |

A missing perspective means **silence**, not a fallback — most commands do not emit to
all three.

Pronoun-substitution schemes (one string, `$n`/`$N`, conjugated per viewer) are
rejected: they drag English grammar — verb agreement, irregulars, reflexives — into
the engine, and any language that restructures the sentence rather than swapping a
pronoun would need the mechanism thrown away. Diku itself kept `to_char`/`to_vict`/
`to_room` as separate strings while having `$n` substitution; the substitution was for
names *within* a perspective.

Three keys also give override granularity: an author can reword the observer line and
leave the actor's alone.

---

## 6. Attempts and facts

**Events that can be denied come in pairs. Events that already happened do not.**

A pair exists for exactly one reason: Bevy guarantees nothing about ordering between
observers of the same event — *"the relative ordering of observers watching for the
same event is arbitrary."* So a mute plugin cannot ensure it runs before the renderer.
A phase boundary is the only fix.

That reason applies only to **attempts**. Facts have nothing to veto, and pairing them
doubles the event surface for a refusal nobody can cast.

| Event | Kind | Paired |
|-------|------|--------|
| `Say`, `Move`, `Damage` | attempt — gag, locked door, immunity | yes |
| `Said`, `Moved`, `Damaged` | fact | — |
| `LoggedIn`, `ConnectionClosed` | fact | no |

**Naming is by tense**: imperative for the attempt, past tense for the fact. `Pre`/
`Post` reads as ceremony and does not say which is authoritative; tense does.

Both phases share one sync point, via `World::trigger_ref` — which runs observers
immediately, unlike `Commands::trigger`, which defers to the next sync point:

```rust
commands.queue(move |world: &mut World| {
    let mut attempt = Say { actor, text, cancelled: None };
    world.trigger_ref(&mut attempt);            // immediate — every vetoer runs now
    if attempt.cancelled.is_none() {
        world.trigger(Said { actor, text: attempt.text });   // possibly rewritten
    }
});
```

Three properties follow:

- **Arbitrary observer order stops mattering.** Cancellation is a monotonic latch, so
  the order vetoers run in cannot change the outcome.
- **A pair costs no extra latency.** Naive `Commands::trigger` per phase would cost a
  sync point per hop.
- **Attempts are mutable, not merely vetoable.** A drunk effect garbles text, a shield
  reduces a number. The fact carries the final values.

### Cancellation carries a Catalog key, not a string

```rust
pub struct CancelReason {
    pub key: &'static str,      // "move.blocked.door_locked"
    pub default: &'static str,  // "The %{dir} door is locked."
    pub args: Vec<(&'static str, String)>,
}
```

A refusal *is* author-facing text, so it uses the same `(key, inline default, args)`
shape as everything else in §5.4. A bespoke `dyn CancelReason` trait would mean
refusals are invisible to string extraction and unoverridable by authors — one
mechanism for all text and a second, incompatible one for refusals.

Being a plain struct also makes refusals `Clone` and serializable, which is what a
moderation log, an admin "why did that fail" tool, and test assertions all need.

If game logic ever needs to *react* to a cause rather than print it — a mob deciding to
unlock the door that blocked it — add `data: Option<Box<dyn Any + Send + Sync>>`. That
is a new field, not a redesign.

**Delivery belongs to the dispatcher**, not the vetoer, so refusal formatting stays in
one place.

**Known nondeterminism:** if two systems veto the same attempt, observer order decides
*which reason* the player sees. The outcome is deterministic — cancelled either way —
and both reasons are true, so the player is not misled. Ranking reasons is ceremony for
a case players cannot detect.

---

## 7. Channels

`say`, `yell`, `ooc`, `gossip`, and clan chat are one mechanism with different
configuration, not five implementations. `grim-channel` owns it, and channels are
**data**:

```rust
app.add_channel(Channel {
    name: "gossip",
    scope: Scope::Global,
    identify: Identify::Always,   // out-of-character: the player speaks, not the character
    toggleable: true,
    speak: Eligibility::All,
    listen: Eligibility::All,
    key: "channel.gossip",
});
```

One call registers the command, audience resolution, the on/off command, and the
Catalog keys. `say` is the same call with `scope: Room`, `identify: Perceived`,
`toggleable: false`.

### Why one event here and not for commands

A single `ChannelMessage { channel, actor, text }` looks like the generic event §5.2
rejects. The distinguishing test is **how many observers care**:

- **Commands** — `say` and `kill` live in unrelated crates. One shared event means
  hundreds of uninterested readers filtering. Rejected.
- **Channels** — audience, visibility, membership, and formatting are genuinely the
  same logic, so one shared event has **exactly one** observer. That is not a
  broadcast; it is a function.

### Axes

| Axis | Values | Note |
|------|--------|------|
| `scope` | `Room`, `Area`, `Global` | earshot only — see below |
| `identify` | `Perceived` / `Always` | `Perceived` consults `name_for`; in-world sound |
| `toggleable` | bool | per-player subscription state, owned by `grim-persistence` |
| `speak` | predicate | who may send |
| `listen` | predicate | who may receive — **separate from `speak`** |
| `key` | Catalog prefix | resolves `.actor` / `.observer` |
| rate limit | — | channels are the spam vector; the existing cooldown is per-command |

`speak` and `listen` are separate because the common cases are asymmetric: newbie chat
lets newbies speak while everyone listens, and one gate cannot express that.

### Scope stays closed

`Scope` has three variants and no `Custom`. Scope covers only what is structurally
indexable — earshot. Anything membership-shaped is `Global` plus a `listen` predicate,
so clan chat is `Scope::Global` + `is_clanmate`.

A `Custom` scope would be a second predicate mechanism doing `listen`'s job, and would
force an unanswerable question at every channel: is clan membership a scope or an
eligibility? Both answers are defensible, which is the mark of a bad seam. `Global`
iterates online players rather than using an indexed lookup; at MUD scale that is
irrelevant, and `Custom` remains available later as a pure optimisation.

**`tell` is not a channel.** Its audience comes from the *input* (`tell bob hi`), so a
`(actor, candidate)` predicate cannot see the target, and it needs a `.target`
perspective no broadcast channel has. Different command shape.

### History

Not persisted. If channel replay arrives later it is in-memory or log-derived, so it
does not constrain the storage model now.

### Scene interaction

"Does a channel message reach me while I am in the Editor?" is already answered by the
Scene **output policy** (§5.3). Channels need no rule of their own.

---

## 8. Current state

Thirteen crates exist: `example-mud`, `grim`, `grim-engine-types`, `grim-color`,
`grim-text`, `grim-command`, `grim-networking`, `grim-networking-telnet`,
`grim-scene`, `grim-world`, `grim-actor`, `grim-channel`, `grim-persistence`.

**Decomposition steps 1–9 are done.** The crate boundaries and dependency
direction now match §4: the facade `grim` depends on the subsystems and offers
`GrimDefaultPlugins`; nothing depends back on the facade. Two design-heavy
redesigns were deliberately deferred rather than done blind — see
"Deferred within the decomposition" below.

### Done

- **`crates/example-mud` is the binary.** `src/main.rs` and `src/seed.rs` moved out of
  the workspace root. Note that the root `Cargo.toml` had no `[package]` section, so
  those files were **never compiled** — the binary this document described was not a
  build target. It is one now.
- **Dead duplicates deleted.** Seven files in `crates/grim/src/` were byte-identical
  to their `grim-engine-types` twins and one (`prelude.rs`) had drifted. None was
  declared in `lib.rs`, so none was ever compiled.
- **`grim-color` extracted** (decomposition step 1). Colour markup, ANSI rendering,
  the palette, and `escape_codes` now live in a Bevy-free, serde-free crate.
  `grim-engine-types::color` re-exports it, so `grim::color::*` resolves unchanged.
- **`grim-text` extracted, `rust-i18n` deleted** (decomposition step 2). The catalog
  (`tr`/`tr!`) moved to a Bevy-free crate depending only on `grim-color`. This
  collapsed **two** parallel string systems into one: `rust-i18n`'s `t!` served two
  plain login keys, the hand-rolled `tr` served two coloured social keys, and both
  read the same `locales/en.json`. That file and the `rust-i18n` dependency are gone;
  the four defaults are inlined in `grim-text`. `grim` re-exports `grim_text::tr` at
  its root so `grim::tr` and `grim::tr!` still resolve. The `Catalog` resource with
  author overrides and `strings/`+`templates/` merge is still deferred to the
  plugin-composition work — `grim-text` is a static lookup for now, behaviour
  unchanged.
- **`grim-command` extracted** (decomposition step 3). The registry moved to a
  Bevy-only crate and is now **generic over the produced command type**
  (`CommandRegistry<C>`), so it carries no game vocabulary — the closed `Command`
  enum stays in `grim-engine-types` and `grim-client` instantiates
  `CommandRegistry<Command>`. Two documented defects are addressed:
  - **Priority is explicit and reorderable.** Resolution is exact-match-first, then
    highest-priority prefix. `register` puts a command at the front (preserving
    last-registered-wins by default); `prioritize`/`deprioritize` move a command
    without renumbering anything. The `OnceLock`/`max(entry_idx)` trie is gone.
  - **Contested prefixes are reported.** `contested_prefixes()` lists every
    abbreviation more than one command answers to; `init_registry` logs each at
    startup, so a plugin silently shadowing `n` surfaces instead of confusing a
    player.

  `CommandRegistry` derives `Resource` and is ready to be inserted, but is still
  held in the `OnceLock` for now: its only caller, `handle_client_input`, sits at
  Bevy's 16-parameter limit, so passing it as a `Res` needs that system split first.
  That, and the typed-per-plugin-event dispatch of §5.2 (which also removes the
  closed `Command` enum), belong with the session-dispatch rework — the enum is still
  pattern-matched by the world/social plugins and by `grim-client` for
  who/where/quit, which need session context an observer would not have.

### Gaps between this document and the code

| Issue | Detail |
|-------|--------|
| `grim-engine-types` is a god-types crate | colour (step 1), `tr` (step 2), command registry (step 3), and wire events + `Connection` (step 4) are out. Remaining: game events, components, validation |
| ~~`grim` owns three plugins~~ | Fixed in step 7 (+8). World/shutdown → `grim-world`, Persistence → `grim-persistence`, Social → `grim-channel`; `grim` is a facade |
| `ChannelPlugin` holds `say`/`yell`/`ooc` as code | still three coded handlers; `add_channel` data model (§7) is deferred with typed-event dispatch |
| No attempt/fact split | `SayEvent`/`MoveEvent` are facts with no cancellable phase, so nothing can veto (§6) |
| System ordering is a single `.after()` chain | `ClientPlugin` chains five systems; split across crates this needs explicit `SystemSet`s, or each dispatch hop costs a frame |
| ~~Dependencies point the wrong way~~ | Fixed in step 9. `grim` depends on the subsystems and re-exports them (`GrimDefaultPlugins`); nothing depends back on the facade |
| ~~`CommandRegistry` is held in a `OnceLock`~~ | Fixed in step 6. It is a Bevy resource, threaded into `handle_client_input` via a `SessionRes` `SystemParam` that keeps the signature within the 16-parameter limit |
| `Command` is a closed enum | downstream authors cannot add variants; replaced by per-plugin event types. `grim-command` (step 3) no longer *depends* on the enum, but grim-client/world/social still pattern-match it — retiring it is part of the §5.2 typed-event dispatch, coupled to the session rework |
| ~~Two i18n systems~~ | Fixed in step 2. `rust_i18n` deleted; one catalog (`grim-text`) serves every key |
| `convert_16color` runs twice | once inside `tr()`, once in the telnet output path. The `tr()` pass is vestigial — it predates the escape and no longer dodges anything. Now merely wasteful rather than wrong: the escape is idempotent across passes. Collapses when the `Catalog` resource lands |
| ~~`include_str!` reaching out of the crate~~ | Fixed in step 2. `locales/en.json` deleted; defaults are inlined in `grim-text` |
| `ClientState` is a single closed enum | no stack, no third-party scenes |
| "Client" means three things | session state machine, wire framing, and terminal impersonation |
| ~~Prefix collisions resolve by link order~~ | Fixed in step 3. Priority is an explicit reorderable ordering (`prioritize`/`deprioritize`), and `contested_prefixes()` is logged at startup so a shadowed `n` is no longer silent |
| ~~Colour codes are not escaped in interpolated values~~ | Fixed. `tr` escapes every argument via `escape_codes`, and `convert_16color` now forwards `{{` instead of resolving it so `ansi` is the escape's only consumer — without that, the second conversion pass undid the escaping. `format_yell`/`format_ooc` escape by hand because they bypass the catalog |

### Done (steps 4–9)

- **Step 4 — `grim-networking`.** The `Connection` component and the five wire events
  moved out of the god-types crate; `ClientInput`/`ClientOutput` were renamed to
  `ConnectionInput`/`ConnectionOutput`. No `Transport` trait yet — a trait against a
  single implementation is the premature-abstraction trap above, so the tokio bridge
  stays in the telnet transport until a second transport is real.
- **Step 5 — `grim-networking-telnet`.** Renamed from `grim-protocol-telnet` and
  repointed off the facade onto `grim-color`, `grim-networking`, `grim-engine-types`.
- **Step 6 — `grim-scene`.** Renamed from `grim-client`; `CommandRegistry` is now a
  real Bevy resource (the `OnceLock` is gone), threaded into the input dispatcher via a
  `SessionRes` `SystemParam` so the 16-parameter limit — the forcing function — is
  resolved.
- **Step 7 — `grim-world`, `grim-persistence`.** Split out of the facade; `grim-world`
  carries the shutdown countdown state, ticking, and SIGTERM bridge. (Placement
  Phase 2a step 2 later moved the admin-gated `shutdown` *command handler* — which
  reads a `Character` — into `grim-actor`; the being-free machinery stays here.)
- **Step 8 — `grim-channel`.** The say/yell/ooc handlers moved out of the facade
  (`SocialPlugin` → `ChannelPlugin`).
- **Step 9 — facade inversion.** `grim` depends on the subsystems, re-exports them, and
  provides `GrimDefaultPlugins`; `grim-scene` and `grim-networking-telnet` no longer
  depend on the facade. `example-mud` depends on `grim` alone.

### Deferred within the decomposition

Two redesigns were kept behavior-preserving rather than done blind, because each
rewrites a contract the current tests assert and neither is a mechanical move:

- **Scene stack (§5.3).** `grim-scene` still uses the `ClientState` enum. The
  entity-per-scene model with `EntityEvent` routing and output policy (Pass/Buffer/Drop)
  — and the `grim-auth` / `grim-editor` split that hangs off it — is a from-scratch
  rewrite of the session loop and lands separately.
- **Typed-event dispatch (§5.2) and channels-as-data (§7).** The closed `Command` enum
  and the distinct `Say`/`Yell`/`Ooc` events are still in place. Retiring them for
  per-plugin event types and a single scope-resolved `ChannelMessage` (the `add_channel`
  data model) is one coupled change, since channel audience resolution currently lives
  behind `grim-scene`'s renderer. It, and the attempt/fact split (§6), follow the scene
  rewrite.

---

## 9. Deferred

Named here so they are not rediscovered as surprises. None is designed yet.

**`grim-perception`** — visibility has two separate checks: *filtering* (do you receive
the message at all — blind, deaf, asleep, out of range) and *identification* (do you
see `Bob` or `someone`). Both are per-(viewer, subject) pairs, which means **a message
cannot be formatted once and broadcast** — rendering is per recipient. If combat,
social, and movement each implement their own invisibility rules they will disagree, so
this belongs in one subsystem, below `grim-world`, exposing roughly `can_perceive` and
`name_for`. Hard rule once it exists: output code never reads the `Name` component
directly.

**Perspective macro** — generating the actor/target/observer family from one prefix,
with source and target given at the call site, rather than three hand-written calls.

**Templates with control flow** — `%{var}` substitution cannot express "for each exit,
render this row"; those loops are in Rust today, so authors cannot reorder a room
display. Additive when needed: same key, same lookup, richer renderer. ⚠️ Mind the
delimiters — `{{` is already the literal-brace escape in colour codes, so a
`{{ var }}` template syntax collides head-on. Both minijinja and tera support custom
delimiters.

**Combat contract split** — `grim-combat` starts as one crate. If a second engine
appears, it splits into a contract (`Health`, damage events, the `attack` command) plus
engines. The payoff is not the swap itself but that spells and items keep working
across it — without a shared contract, a spell plugin cannot deal damage without
knowing which engine is installed.

**Channel rate limiting** — channels are the spam vector and the existing cooldown is
per-command, not per-channel.

**`grim-emote`** — socials (`smile`, `bow`) share perception, perspective, and Catalog
machinery with channels but are not channels: no free text, a fixed template per emote,
and typically hundreds loaded from a data file.

**`tell`** — audience comes from the input rather than configuration, and it needs a
`.target` perspective, so it does not fit `Channel`.

**Palette as runtime config** — `palette.rs` is compile-time constants today, which
keeps `grim-color` free of Bevy. Making the palette author-configurable means a
resource, a Bevy dependency, and probably a plugin. Deferring costs nothing; adding
the dependency now is hard to walk back.

**Author-coloured arguments** — `escape_codes` is applied to every interpolated
value, so an argument can never carry markup. If a legitimate case appears (a
coloured clan tag, say), it wants an explicit opt-in at the call site. Deferred until
one exists, because the default has to stay "escape", and an opt-in added later is
cheap while a hole closed later is not.

---

## 10. Testing

Everything GRIM does is a plugin, and plugins are registered rather than
hardcoded. That is what makes the engine extensible — and it is also the thing
most likely to break silently, because two independently-correct plugins can
still conflict once composed. Testing has two layers, and the second exists
specifically to catch that.

### Unit tests — a crate in isolation

Each crate tests its own logic with no `App`, or a minimal one:
`grim-color`/`grim-text` are pure functions; `grim-command` resolution,
`grim-scene`'s parser, and the shutdown countdown are exercised directly. These
are fast, deterministic, and prove a single piece correct. What they *cannot*
see is interaction: a unit test of `grim-command` never learns that
`grim-channel` and a world command both answer to `n`.

### End-to-end tests — the whole engine composed

`crates/example-mud/tests/` boots the **real** stack a player would hit and
drives it from the outside. The harness (`tests/harness`) is deliberately
**message-level, not socket-level**: it adds `GrimHeadlessPlugins` — the exact
plugin set `GrimDefaultPlugins` ships, minus the transport — injects
`ConnectionInput`, and records every `ConnectionOutput` per connection. It uses
no TCP and advances the clock manually, so runs are deterministic and free of
ports and sleeps. State is isolated per test via `PersistenceConfig` pointed at a temp dir,
so every test starts from a clean world.

Two properties make this the plugin-conflict harness:

- **It composes the shipping plugin set.** `GrimHeadlessPlugins` is the shared
  core of `GrimDefaultPlugins`, so the E2E stack cannot drift from what the
  binary runs. If two plugins collide, the collision is present in the test.
- **It asserts on player-visible output**, not internal state. A scenario says
  "create an account, make a character, `say hello`, and another player in the
  room hears it" and checks the recorded lines. That exercises command
  resolution, scene routing, channel audience, the Catalog, and persistence in
  one pass — the seams *between* plugins, which is where composition breaks.

Conflicts this layer is meant to surface, and unit tests structurally cannot:

- **Contested command prefixes / shadowed verbs** — a new plugin's command
  stealing an abbreviation another plugin relies on.
- **Duplicate or missing registration** — two plugins registering the same
  resource or message, or a plugin assuming a message another plugin was
  supposed to register.
- **System-ordering hazards** — a dispatch hop that only works because of an
  incidental `.after()` ordering, or a deferred `commands.spawn` read in the
  same frame before it is flushed.
- **Cross-plugin visibility leaks** — e.g. one account seeing another's
  characters; there is an E2E regression test for exactly this.

The rule of thumb: **if a behaviour depends on more than one plugin, it wants an
E2E scenario.** As third-party plugins register commands, scenes, and channels,
this is the layer that proves they coexist. When a subsystem grows (combat,
items), it adds scenarios here rather than only unit tests, and the harness gains
an admin/immortal session helper to set a scene up (spawn a mob into a room)
before driving a player through it.

### Real-network tests — when message-level cannot reach

A few behaviours are *inherently* socket- and process-level and cannot be
exercised by the headless harness. **Copyover** (below) is the current example:
it forks + execs the compiled binary and passes live socket fds over a unix
socket. `crates/example-mud/tests/copyover.rs` therefore spawns the real
`copyover_fixture` binary, connects a real `TcpStream`, and asserts the same
socket survives a `SIGUSR2` restart. This tier is Unix-only and slower; reach for
it only when a feature genuinely crosses the process/socket boundary — otherwise
the deterministic message-level harness is preferred.

## 11. Copyover (hot restart)

A copyover swaps the server binary **without disconnecting players**. `SIGUSR2`
makes the running process exec the freshly-deployed binary and hand its live
listener + in-game client sockets to the successor over a unix socket
(`SCM_RIGHTS`, via the `sendfd` crate). The successor reloads the world from
scratch, then re-adopts each socket and drops its character straight back into
the world — no login — at the room it was standing in.

- **World is scratch, persistent data carries.** Only accounts/characters survive
  (reloaded from disk). A character's room is persistent: `grim-world` keeps
  `Character.last_room` current on every move and `grim-persistence` writes it to
  disk each step, so the successor's disk load already has the right room.
- **Who migrates.** Only actively-playing sessions (in-game, not linkdead).
  Login-limbo sockets are dropped and reconnect fresh. On the far side, any
  character that cannot be cleanly rehydrated has its socket dropped rather than
  logged into a corrupt state (fail closed).
- **systemd.** The unit is `Type=notify`/`NotifyAccess=all`; the successor sends
  `MAINPID` before the predecessor exits, so systemd follows the handoff instead
  of treating it as a crash. `SIGTERM` is the *graceful shutdown* trigger (warned
  countdown), distinct from `SIGUSR2`.
- **Boundaries.** `grim-networking` gained only game-agnostic wire types
  (`HandoverManifest`, `ConnectionResumed`); the fd plumbing + process handoff
  live in the telnet transport, and the resume placement lives in `grim-scene`.
- **Deferred.** Durable persistence (WAL + dirty-flag/timer autosave) so an
  *unexpected* crash loses little; today the flush points are disconnect, quit,
  and move. See `docs/DEPLOY.md`.
