# Character gender, race, and class tiers

status: accepted

First-pass character identity: a new character picks a **gender**, a **race**,
and a **class**, and starts at **level 1**. This ADR records how those three are
modelled — a closed enum for gender, registered *data* for race and class — and
why the class registry encodes a **tier ladder** whose evolution mechanic is
deliberately left unbuilt.

**Evolution/reroll is NOT being implemented now.** What gets built is the
creation flow (gender → race → class → level 1) and the data that describes the
ladder. The tier-2 classes are seeded so the forward path is visible and
non-blocking, but nothing promotes a character between tiers yet, and there is
no experience/XP system — level is just a stored number.

## Terminology

Defined in [CONTEXT.md](../../CONTEXT.md): **Gender** (closed enum), **Race** and
**Class** (registered data addressed by **Slug**), **Class Tier** (tier-1
creatable, tier-2 via a future reroll). A race/class **Slug** here is the same
kind of human-facing, lower-case, hyphenated identifier used for areas/rooms —
`half-elf`, `warrior` — but it keys a build-data registry, not a world record.

## Gender is a closed enum

`Gender { Male, Female, Neutral }` — a fixed, exhaustive set, `Default =
Neutral`. Unlike race and class it is *not* data: a plugin should be able to
match it exhaustively (pronoun selection, gendered emotes), and the set is small
and stable enough that authoring new genders is not a goal of the first pass. It
serializes lowercase (`"male"`), exactly like `Role`, so character JSON stays
hand-editable.

## Race and class are registered data, not code

A `RaceDef` / `ClassDef` is a plain record — `slug`, `name`, `abbrev`,
`description` (plus, for a class, `tier` and `evolves_to`). The playable sets
live in `RaceRegistry` / `ClassRegistry` resources, each with a seeded `Default`
(7 races, 10 classes). This mirrors `ReservedNamePrefixes`: `ScenePlugin` calls
`init_resource`, so the engine works out of the box, and an author overrides the
set by inserting their own registry *before* adding the plugin. The creation
menus are built from whatever the registries hold, so adding a race or class is
a data edit, not a code change.

The character stores only the **slug** (`race: String`, `class: String`), keyed
back into the registries for display. Storing the slug rather than the whole
record keeps the character record small and lets an author reword a
name/description without rewriting saved characters.

## The class registry is a tier ladder

Classes carry a `tier: u8` and an `evolves_to: Option<String>`:

- **Tier 1** (warrior, mage, cleric, thief, ranger) are the only **creatable**
  classes — the creation menu lists `tier == 1` exclusively. Each names its
  tier-2 successor via `evolves_to` (`warrior` → `champion`, …).
- **Tier 2** (champion, archmage, templar, assassin, warden) are seeded but
  **not** creatable (`evolves_to: None`). They exist so the ladder is fully
  described in data today.

The intended future is **rerolling**: a tier-1 character evolves into its tier-2
class. Because a class is just a slug on the character, a reroll is a single
field swap — `class = old.evolves_to`. Later tiers (tier-3 "hero" or
specialization classes) extend the ladder additively: give the tier-2 defs their
own `evolves_to`, seed the tier-3 defs. None of that is built yet.

## Why a single `class` field + a data-driven ladder

For the first pass a character holds **one** `class` slug. Alternatives
considered and rejected *for now*:

- **Tier-tagged slots** (`class_t1`, `class_t2`, …, or a `Vec<(tier, slug)>`) —
  premature: nothing reads a history of past classes yet, and reroll semantics
  (does the tier-1 identity survive?) are undecided. A single field defers that
  decision without blocking it: the field can grow into tagged slots additively
  via `#[serde(default)]`, exactly as `roles`/`gender`/`race`/`class`/`level`
  were added to `Character` without breaking old JSON.
- **An enum of classes in code** — would make "add a class" a code change and a
  recompile, contradicting GRIM's "everything registered" stance. The ladder
  (which class evolves into which) is precisely the kind of author-tunable
  configuration that belongs in data.

A single slug plus a data ladder is the smallest thing that (a) makes creation
work today, (b) states the intended evolution in data, and (c) leaves every
harder decision (reroll semantics, multi-tier storage, XP) to be made later
without a migration.

## Creation flow

The closed `ClientState` machine gains three states threaded with the
accumulated picks:

```
CreateCharacter(name)
  -> SelectGender { name }
  -> SelectRace  { name, gender }
  -> SelectClass { name, gender, race }
  -> persist Character { name, gender, race, class, level: 1, .. }
  -> MotdPrompt -> InGame          (existing path, unchanged)
```

Each picker prompts a numbered menu and accepts either a 1-based index or a
case-insensitive name/slug prefix; invalid input re-prompts without advancing.
The class menu lists only tier-1 classes. The character is **not** persisted
until a class is chosen — an abandoned creation leaves nothing on disk.

## Consequences

- `Character` gains `gender`, `race`, `class`, `level`, all `#[serde(default)]`
  (level defaults to 1), so pre-existing character JSON loads unchanged
  (neutral, no race/class, level 1).
- Races/classes are overridable per MUD by inserting a registry before
  `ScenePlugin`; the seeded defaults ship a complete, playable set.
- Reroll/evolution, tiers beyond the seeded two, and any XP/levelling mechanic
  are explicitly out of scope. The tier-2 data and the single `class` field are
  the seams that make adding them later additive rather than a rewrite.
