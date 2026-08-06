# GRIM

GRIM is a foundation for building MUDs, not a MUD. Every capability it offers is a
Bevy plugin, and every plugin is an extension point that third parties can register
against. This glossary fixes the language for those extension points so that crate
names, component names, and event names all agree.

## Language

### Naming

**Subsystem**:
A top-level capability of the engine that other things register against. Named
`grim-<subsystem>`, one plugin per crate.

**Extension**:
An implementation that registers against exactly one subsystem. Named
`grim-<subsystem>-<extension>`, so the crate name states what it plugs into.
_Avoid_: `grim-core-*` — the `grim-` prefix already marks a crate as ours, so
`core` carries no information and invites an argument at every new crate.

### Connection and session

**Connection**:
A live transport-level link between a remote party and the server. Owns the
socket and nothing about the game.

**Protocol**:
The rules for turning a **Connection**'s bytes into lines of text and back.
Telnet, SSH, and WebSocket are each a Protocol.

**Session**:
Who is currently driving a **Connection** — the account, the character, and the
current **Scene**. One Session per Connection, held on its own entity so the game
never touches transport types.
_Avoid_: Client.

**Scene**:
A way of interpreting one line of input — a username, a password, a `y/n`, a menu
index, an in-game **Command**, a line of prose being edited. Scenes are registered
by plugins, so a MUD author can add their own.
_Avoid_: ClientState, session state, screen, mode.

**Scene stack**:
The ordered set of **Scenes** a **Session** currently occupies. Input is always
interpreted by the topmost Scene. Scenes are pushed and popped rather than
switched, so a player can be in the Editor while still standing in the world.
_Avoid_: Modal — a layered Scene is not a distinct kind of thing, so a second word
for it would only raise the question of which one to use.

**Output policy**:
What a **Scene** does with game output addressed to a **Session** while that Scene
is on top — pass it through, buffer it until the Scene is popped, or drop it. The
Editor buffers, so a room conversation cannot splatter into the text a player is
composing.

### Input and dispatch

**Command**:
A single intent submitted by an actor, expressed as one line of text
(`kill goblin`). A Command is not inherently tied to a **Connection** — a mob AI,
an admin console, or a test may submit one with no Connection in existence.
Deliberately chosen over "verb" because Command is what a MUD author already calls
it; see "Flagged ambiguities".

**Router**:
The subsystem that resolves the first word of a **Command** to the plugin that
registered it, and hands the remainder to that plugin. The Router knows nothing
about transports.

**Actor**:
The entity a **Command** is attributed to. Usually a character, but not
necessarily. The "beings" that can act and be placed in a room — `Character`,
`Player`, `InRoom`, `Linkdead`, `OutputHistory`, `Role` — live in the
`grim-actor` crate as of Placement Phase 2a step 2, above the being-free
`grim-world`. (This step only *relocated* `Character`; splitting it into an
`Actor`/`Creature`/`Character` hierarchy is step 3.)

**Alias**:
A player-defined shorthand that expands to one **Command** before the **Router**
sees it. Expanded exactly once, so an Alias can never expand into another Alias.

**Priority**:
The order the **Router** prefers **Commands** in when a player types a prefix that
several of them share. Defaults to registration order; a MUD author reorders it
explicitly rather than by shuffling plugin registration.

### Actions

**Attempt**:
A requested action that something may still deny — speaking, moving, dealing damage.
Named in the imperative (`Say`, `Move`, `Damage`). An Attempt can be modified as well
as denied, so a drunk effect may garble text rather than block it.

**Fact**:
An action that has happened. Named in the past tense (`Said`, `Moved`, `Damaged`) and
carries the final values after any Attempt was modified. Nothing can deny a Fact, so
Facts have no paired Attempt.

**Cancel reason**:
Why an **Attempt** was denied, expressed as a **Key** with arguments so the player
sees a localized, author-overridable message. Delivered by the subsystem that owns the
Attempt, not by whatever denied it.

### Communication

**Channel**:
A configured audience for player speech — `say`, `yell`, `ooc`, `gossip`, clan chat.
Channels differ only in configuration, never in implementation: who hears it, whether
the speaker is identified, whether it can be switched off, and who may speak or listen.

**Scope**:
Which entities are within earshot of a **Channel** — the room, the area, or everyone.
Scope covers only structural reach; permission to speak or listen is separate, so
clan chat is a global Scope with a membership check rather than a Scope of its own.

**Eligibility**:
Whether a character may speak on, or listen to, a **Channel**. Speaking and listening
are asked separately, because newbie chat lets newcomers speak while everyone listens.

### Presentation

**Catalog**:
Every piece of author-facing text in a running MUD, addressed by **key**. Plugins
contribute defaults; the MUD author overrides any key without forking the plugin
that owns it. One namespace, regardless of how a given entry is stored.

**Key**:
The dotted name a piece of text is addressed by (`combat.damage.hit`). Stable
across rewording, which is what makes overriding possible.

**String**:
A single-line **Catalog** entry, stored in a merged set of JSON files under
`strings/<locale>/`.

**Template**:
A multi-line **Catalog** entry — a layout block such as a room display or a
character sheet — stored one file per **Key** under `templates/<locale>/`.
The distinction from a **String** is where it is filed, not how it is looked up;
both resolve through the same **Catalog**.

**Colour code**:
GRIM's transport-independent markup for styled text. Each **Protocol** decides
what to do with it: telnet and SSH translate it to ANSI escape sequences,
WebSocket passes it through for a browser client to interpret.
_Avoid_: ANSI (that is one specific rendering of a colour code, not the concept).

### Identity

Four distinct identifiers, from most to least specific:

**Entity ID**:
A Bevy `Entity` — the runtime handle for one live thing. Truly unique, cannot
collide, but only valid for the current boot (changes across reboots/copyover).
Never persisted. The most specific way to address an **Instance** or a live room.

**Grim ID**:
The immutable identity of a persistent record — an account, character, or a
**Blueprint**'s area and rooms. A short `nanoid` over a **base62** alphabet
(`A-Za-z0-9`, length 12), generated once at creation, never changed. Globally
unique. Persisted. A room's Grim ID lives in its Blueprint, so every **Instance**
stamped from that Blueprint shares it. The base62 alphabet has no `-`/`_`, so a Grim
ID never shares a shape with a **Slug** (which uses hyphens).
_Avoid_: Uuid — replaced by the shorter nanoid.

**Slug**:
A human-readable, author-facing alias, usually the filename (`haven`,
`market-square`). NOT unique — many Instances of one Blueprint share a Slug, and it
can be renamed. Lower-case, web-style. Was called "Friendly ID".
_Avoid_: Friendly ID (retired), Name (a character's Name is really its Slug, but the
word Name stays for display).

**Instance ID**:
Distinguishes one non-**Canonical** **Instance** of a Blueprint from another.
Generated at instance creation, same shape as a **Grim ID** (base62, length 12,
globally unique) but a different role — Grim ID is the *record/Blueprint* identity
(shared by every copy), Instance ID is *this live copy's* identity. Persisted (it is
the `instances/<instance-id>.json` filename and the area part of a character's
`last_room`). The **Canonical** Instance carries none — its Blueprint area Grim ID
already resolves to it. So an instanced area holds both: the shared Blueprint area
Grim ID and its own unique Instance ID.

**Address**:
How input (e.g. the admin `goto` command) names a target room. One of:
- an **Entity ID** (single boot-local token), or
- `<area>:<room>`, where each side is independently a **Grim ID** or a **Slug** (any
  combination), and the area side may also be an Entity ID, or
- a bare room token (**Grim ID** or **Slug**).
Resolution precedence when a bare/slug token is ambiguous is an implementation
concern, not glossary — see
[ADR-0001](docs/adr/0001-area-identity-and-instancing.md).

### Character build

The three choices a player makes at creation, plus the class ladder they sit on.
See [ADR-0002](docs/adr/0002-character-class-tiers.md).

**Gender**:
A character's gender — a **closed** set (`Male`, `Female`, `Neutral`; default
`Neutral`), not data. Because it is closed, a plugin can match it exhaustively
(pronouns, gendered emotes). Contrast **Race**/**Class**, which are open data.
_Avoid_: sex, pronoun (pronoun is a *rendering* of a Gender, not the concept).

**Race**:
A playable ancestry — registered *data* addressed by a **Slug** (`human`,
`half-elf`). A `RaceDef` carries slug, name, abbrev, and description; the set
lives in the `RaceRegistry` resource with a seeded default an author overrides.
The character stores only the slug.

**Class**:
A playable vocation — registered *data* addressed by a **Slug** (`warrior`,
`mage`), held in the `ClassRegistry`. Like a Race it carries slug/name/abbrev/
description, and additionally a **Class Tier** and an `evolves_to`. The character
stores a single class slug for now.
_Avoid_: profession, job — settled on Class, the word a MUD author uses.

**Class Tier**:
A class's rung on the evolution ladder. **Tier-1** classes are the only ones
**creatable** — offered at character creation. **Tier-2** classes are seeded but
reachable only via a future **reroll** (each tier-1 names its tier-2 successor in
`evolves_to`). Reroll/evolution and any tier beyond the seeded two are NOT
implemented yet; the ladder is described in data so adding them stays additive.

### World instancing

**Blueprint**:
The on-disk definition of an area (and its rooms), e.g. `haven.json`. Not in the
world — it is stamped into the world to produce **Instances**. A Blueprint is
addressed by its **Slug** or **Grim ID** (see Identity).
_Avoid_: Template — that is a **Catalog** entry (author-facing text), an unrelated
concept. _Avoid_: Prototype, Definition — settled on Blueprint.

**Instance**:
A live copy of a **Blueprint** in the ECS. Every area in the running world is an
Instance, whether one exists or many. An Instance has an **Entity** and, if it is
not the **Canonical** one, an **Instance ID**.
_Avoid_: "concrete area" as a distinct kind — it just means the Canonical Instance.

**Canonical**:
The single **Instance** of a Blueprint that cross-area linkages resolve against. A
marker, not a separate kind of thing — exactly one Canonical Instance per Blueprint.
Exits between areas bind **Canonical → Canonical** only: they seek each other by
Blueprint identity at load. A non-Canonical (instanced) copy still binds its
*outbound* exits to Canonical targets, but no Canonical exit ever points *into* an
instanced copy — walking back lands you in Canonical, not your instance, until
explicit instance-aware exit logic exists (not yet).

## Flagged ambiguities

**"Client" is retired.** It was carrying three unrelated meanings at once:

1. the session state machine (`Client` component, `grim-client` crate) → now **Session**
2. wire framing (`ClientInput` / `ClientOutput`) → now `ConnectionInput` / `ConnectionOutput`
3. the user's terminal, which telnet and SSH must impersonate because those
   protocols are too thin to do it themselves → no longer a named concept; the job
   lives inside the relevant **Protocol** extension

Because meaning 3 is genuinely a different job from meaning 1, keeping the word for
either one would leave every reader guessing which was meant.

**"Command" collides with Bevy, and we accept it.** Bevy's `Commands` is a deferred
world-mutation buffer and its `Command` is one unit of work on that buffer. GRIM's
**Command** is a player verb. These are unrelated, and they will appear in the same
file.

We keep **Command** anyway, because the audience that matters is the MUD author
calling `add_command`, and "command" is the word they already use. The alternative
(Verb) optimised for the engine's internals at the expense of its public surface.
When ambiguity threatens internally, write "Bevy `Commands`" for the buffer and plain
"Command" for the player verb.

## Example dialogue

**Dev:** Someone telnets in and types `say hello`. Walk me through who owns what.

**Domain expert:** A **Connection** appears — that is just the socket. The telnet
**Protocol** frames the bytes into the line `say hello`. A **Session** is attached to
that Connection, and its **Scene** is in-game, so the Session hands the line to the
**Router**.

**Dev:** And if the Scene were the login prompt instead?

**Domain expert:** Then the Session never involves the Router at all — the line is a
username, not a **Command**. That is the whole point of the Scene: it decides what a
line of input even *means*.

**Dev:** So the Router only ever sees in-game input?

**Domain expert:** The Router only sees Commands. Whether a Command came from a
Session, from a mob's AI, or from a test is not something the Router can tell, and it
should not be able to.

**Dev:** Right — `say` resolves to the social plugin. What about the colour in the
reply?

**Domain expert:** The social plugin writes **colour codes**. It has no idea that a
telnet Connection is on the other end. The telnet Protocol turns those into ANSI. A
WebSocket Protocol would pass them straight through and let the browser style them.
