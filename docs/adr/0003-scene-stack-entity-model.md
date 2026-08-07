# Scene-stack entity model

status: Proposed

> ⚠️ **Needs maintainer decision — do not implement until accepted.** This ADR
> is a design proposal for one of the deferred §8 redesigns in
> [ARCHITECTURE.md](../ARCHITECTURE.md). It records a recommended direction for
> review; no code should change on its basis until it is marked `accepted`.

`grim-scene` still interprets input through a single closed `ClientState` enum
(ARCHITECTURE.md §8, "Deferred within the decomposition"). ARCHITECTURE.md §5.3
and CONTEXT.md already fix the *target* — a **Scene stack** of pushable entities —
but the code has not moved there. This ADR chooses the entity/data model for that
stack, how login/creation/select/MOTD (now in `grim-auth`), the editor
(`grim-editor`), and in-game play become Scenes, how input routes to the top
Scene, and where the stack lives.

## Terminology

Defined in [CONTEXT.md](../../CONTEXT.md): **Session** (who drives a Connection),
**Scene** (how one line of input is interpreted), **Scene stack** (the ordered set
of Scenes a Session occupies; input goes to the top), **Output policy**
(Pass/Buffer/Drop for game output while a Scene is on top). _Avoid_: ClientState,
mode, screen, modal.

## Problem with `ClientState`

A single closed enum has three defects the target names:

1. **It cannot layer.** "In the Editor while still standing in the world" is not
   representable — the `Playing` state would have to be smuggled inside the
   `Editor` variant (ARCHITECTURE.md §5.3).
2. **It is closed.** A third-party plugin cannot add a login step, a shop haggle
   prompt, or a `y/n` confirmation without editing an enum in a crate it does not
   own. This is the same closed-enum extensibility failure §5.2 calls out for
   `Command`.
3. **It concentrates unrelated logic.** ADR-0002's creation flow already had to
   thread `SelectGender`/`SelectRace`/`SelectClass` as new enum variants carrying
   accumulated picks — every new prompt grows one enum and the match arms that
   dispatch on it.

The **current two-system `JustEnteredWorld` guard is the seam this replaces**: it
exists only because entering the world is a transition the flat state machine
cannot express as a push, so an extra system watches for the edge. A stack makes
"entered the world" the ordinary act of pushing the in-game Scene.

## Options

### Option A — keep the enum, add sub-states

Extend `ClientState` with richer variants and helper transitions. Zero new
concepts.

- **+** No migration; smallest diff.
- **−** Keeps every defect above. Layering still impossible; still closed; still
  one match per input. Rejected by §5.3 already — listing it only to close it.

### Option B — a Scene stack of entities (RECOMMENDED)

Each Scene is an **entity** carrying a marker component and its own data. The
Session entity holds an ordered `SceneStack(Vec<Entity>)`. Input arrives as an
`EntityEvent` targeted at the top entity, so only that Scene's observer runs — the
same flat-cost dispatch commands get in §5.2. Plugins register Scenes and push
them:

```rust
#[derive(Component)]
struct LoginScene { stage: LoginStage }

// grim-auth pushes its first scene when a Connection is established
commands.entity(session).with_scene(LoginScene { stage: Stage::Username });
```

- **+** Layering is native: the editor pushes over in-game and pops to reveal it.
- **+** Open: a third-party plugin defines a Scene component + observer and pushes
  it; no engine enum to edit.
- **+** Per-Scene data lives on the Scene entity, not smeared across one enum's
  variants. ADR-0002's creation picks become fields on a `CreationScene`.
- **+** Output policy is a field/component on the Scene entity — the dispatcher
  reads the top Scene's policy to Pass/Buffer/Drop (§5.3).
- **−** A rewrite of the session loop and its tests (the deferral note in §8 says
  as much). Input routing changes from `match state` to observer dispatch.
- **−** Scene lifetime is now entity lifetime: popping must despawn (or the stack
  leaks entities), and a Session despawn must cascade its Scenes.

### Option C — component-per-scene on the Session entity

No separate Scene entities; each Scene is a component on the **Session** entity,
and a `TopScene` marker/order field says which is active.

- **+** No child-entity lifecycle; everything hangs off one entity.
- **−** Cannot hold **two Scenes of the same kind** (two nested confirmations, an
  editor opened from within an editor) — a component type is single-instance per
  entity. The stack degenerates to "at most one of each Scene type," which is a
  weaker model than §5.3 promises.
- **−** "The top Scene" becomes a hand-maintained ordering field over components
  rather than the natural top of a `Vec`, re-introducing exactly the bookkeeping a
  stack removes.

## Recommendation

**Option B — a stack of Scene entities.** It is the only option that satisfies all
three properties §5.3/CONTEXT.md already commit to: real layering, open
third-party Scenes, and per-Scene data/output-policy. Option A keeps every named
defect; Option C cannot express duplicate-kind layering, which is the whole reason
a *stack* was chosen over a *field*.

Concretely:

- **Where it lives:** the stack, the `EntityEvent` input routing, and the
  Pass/Buffer/Drop output policy live in **`grim-scene`** — it is already "the
  bridge between networking and commands" (§5.3) and owns the Session.
- **Auth/editor/in-game as Scenes:** `grim-auth` registers and pushes the
  login/account-creation/character-select/MOTD Scenes (it already "registers five
  scenes" per §3); `grim-editor` pushes the editor Scene (Buffer policy);
  `grim-scene` (or `grim-actor`) pushes the in-game Scene, whose observer forwards
  the line to the Router. Copyover resume (§11) pushes the in-game Scene directly,
  skipping login.
- **Transitions:** `push_scene(SceneDef)` / `pop_scene`; a Scene pops itself when
  its work completes (login succeeds → pop login, push in-game). The
  `JustEnteredWorld` two-system guard is deleted — entering the world is a push.
- **Input routing:** `ConnectionInput` → `grim-scene` resolves the Session → fires
  an `EntityEvent` at the top Scene entity → that Scene's observer interprets the
  line (password, menu index, or hand-off to the Router).

## Interaction with other proposals

- **ADR-0004 (typed dispatch):** the in-game Scene's observer is the caller that
  invokes the command Router; both use `EntityEvent`/observer dispatch, so the
  session loop and the command loop share one mechanism rather than two.
- **ADR-0005 (render pipeline):** a Scene's output policy is the point where
  Buffer/Drop is enforced, so the render pipeline emits into a per-Session sink
  the top Scene governs.
- **ADR-0007 (`grim-input`):** if input parsing (line editing, telnet vs
  websocket framing quirks) grows beyond `grim-scene`, it splits into `grim-input`;
  this ADR does not require that split, only leaves room for it.

## Consequences

- `grim-scene` gains the stack, scene-registration API, and observer-based input
  routing; `ClientState` and `JustEnteredWorld` are removed.
- Scene entities have a lifecycle: pop despawns; Session despawn cascades. E2E
  tests (ARCHITECTURE.md §10) that assert on player-visible output at each prompt
  stay valid, but the unit tests that assert on `ClientState` transitions are
  rewritten to assert on stack contents.
- Third parties can add prompts/steps without touching engine crates — the closed
  creation flow of ADR-0002 becomes an open, pushable sequence.
- **Open question for the maintainer:** should a Scene be *one* entity per Session
  occurrence (B as written), or a shared Scene-definition entity plus per-Session
  cursor state? B-as-written is simpler and matches "a Scene is an entity carrying
  its own data" (§5.3); the shared-def variant saves entities at the cost of
  splitting definition from state. Recommend B-as-written unless entity count at
  MUD scale is shown to matter (it will not).
