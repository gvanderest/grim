# GRIM Engine — Agent Guide

A modular MUD engine on Bevy ECS (0.19). Everything is an entity, every interaction is an event, the server is a binary composing plugins.

## Architecture (4 event-passing layers)

```
Protocol  ─→  Client  ─→  Engine  → Persistence
  bytes        state       ECS        disk
```

**No layer calls another's functions or reads its components.** Communication is exclusively through Bevy `Event` (here `Message`) types.

| Layer | Crate | Responsibility | Output |
|---|---|---|---|
| Protocol | `grim-protocol-telnet` | Raw TCP, telnet IAC negotiation, tokio bridge | `ClientInput`, `ConnectionEstablished` |
| Client | `grim-client` | Session state machine, command parser, output formatter | `EngineCommand` |
| Engine | `grim` (`plugins/`) | ECS systems: rooms, movement, social channels, persistence | engine event → client formatting |
| Persistence | `grim::plugins::persistence` | Load accounts/characters from disk, save on disconnect | — |

## Crate Map

| Crate | Purpose |
|---|---|
| `grim` | Engine library: components, events, cardinals, validation, plugin systems |
| `grim-client` | Session lifecycle, input parsing, output formatting |
| `grim-protocol-telnet` | TCP server, IAC negotiation, tokio↔Bevy bridge |
| `mud-example` (root) | Binary: composes plugins, seeds world |

## Key Types

### Core Components (`grim::components`)

| Component | Role |
|---|---|
| `Connection` | Raw socket, owned by protocol layer — engine never touches it |
| `Client` | Session state machine (Login→Password→CharSelect→InGame) |
| `Account` | Persisted to `data/accounts/<uuid>.json` |
| `Character` | Persisted to `data/characters/<name>.json`, tracks `last_room` |
| `Player` | Marks player-controlled character, links `connection: Entity` for output routing |
| `Linkdead` | Character in-world but player disconnected |
| `Room` | `title`, `description`, `area` reference |
| `Exits` | `HashMap<Cardinal, Entity>` — exits on a room |
| `InRoom` | Component on characters/NPCs — `room: Entity` |
| `Area` | Groups rooms for `yell` range |
| `Name` / `Description` | Shared display components |
| `OutputHistory` | Circular buffer — linkdead replay on reconnect |
| `StartingRoom` | Resource — where new characters spawn |

### Cardinal Directions (`grim::cardinal`)

Enum: `North`, `East`, `South`, `West`, `Up`, `Down`. Parses abbreviations (`n`/`e`/`s`/`w`/`u`/`d`). Has `opposite()` for bidirectional room wiring.

### Event Flow (critical pattern)

```
Protocol (tokio)           Client/Bevy              Engine/Bevy
─────────────              ───────────              ───────────
read socket bytes  ──→  `ClientInput` msg      `EngineCommand` msg
                        parse_input() ──────→  `handle_look()`
                                               `handle_move()`
                                               `handle_say/yell/ooc()`
                                               `handle_quit()`

Protocol                                       Engine
────────                                       ──────
`ClientOutput` msg  ←── format_output()  ←──  `LookRoom`, `SayEvent`,
                                               `MoveEvent`, `InfoMessage`, etc.
```

Events are Bevy `Message` types (Bevy 0.19's typed message API, the evolution of `Event`). Writers use `MessageWriter<T>`, readers use `MessageReader<T>`.

### Key Events (`grim::events`)

| Event | Direction | Trigger |
|---|---|---|
| `ConnectionEstablished` | Protocol→Client | Telnet handshake done |
| `ClientInput` | Protocol→Client | Raw line from socket |
| `ConnectionClosed` | Protocol→All | Socket dropped |
| `ClientOutput` | Any→Protocol | Text to send + optional ECHO flag |
| `DisconnectRequest` | Client→Protocol | Clean socket close |
| `EngineCommand` | Client→Engine | Parsed command (`Command` enum) |
| `LookRoom` | Engine→Client | Format room description |
| `LookEntity` | Engine→Client | Format entity description |
| `SayEvent`/`YellEvent`/`OocEvent` | Engine→Client | Social channel broadcasts |
| `MoveEvent` | Engine→Client | Room transition broadcast |
| `InfoMessage` | Engine→Client | Direct text to one character |
| `LoginAnnounce`/`LogoutAnnounce`/`LinkdeadAnnounce` | Engine→Client | Global player announcements |

### Command Enum (`grim::events::Command`)

```rust
pub enum Command {
    Look { target: Option<String> },
    Move { direction: Cardinal },
    Say { text: String },
    Yell { text: String },
    Ooc { text: String },
    Who,
    Where,
    Commands,
    Quit,
}
```

### Client State Machine

```
LoginPrompt → PasswordPrompt → CharacterSelect → MotdGate → InGame
                                                ↑            │
                                                └──── quit ──┘ (back to CharacterSelect)
```

## File Layout

```
├── Cargo.toml                         # Workspace root + binary
├── src/
│   ├── main.rs                        # Plugin composition, app.run()
│   └── seed.rs                        # Bootstrap world (rooms, exits, NPCs)
├── crates/
│   ├── grim/src/
│   │   ├── lib.rs                     # Module exports
│   │   ├── color.rs                  # Color markup → ANSI escape conversion
│   │   ├── components.rs              # ECS components
│   │   ├── events.rs                  # Event/Message types
│   │   ├── cardinal.rs                # Direction enum + parse/opposite
│   │   ├── validation.rs              # Email/name/password validation, SHA-256 hashing
│   │   ├── prelude.rs                 # Re-exports everything
│   │   └── plugins/
│   │       ├── mod.rs
│   │       ├── world.rs               # look, move, quit systems
│   │       ├── social.rs              # say, yell, ooc systems
│   │       └── persistence.rs         # Load on startup, save on disconnect
│   ├── grim-client/src/
│   │   ├── lib.rs                     # ClientPlugin, input dispatch, formatting routing
│   │   ├── parser.rs                  # Raw text → Command
│   │   └── formatter.rs              # Engine events → formatted text
│   └── grim-protocol-telnet/src/
│       └── lib.rs                     # TelnetPlugin, tokio acceptor, IAC negotiation
├── data/
│   ├── accounts/<uuid>.json           # Persisted accounts
│   └── characters/<name>.json         # Persisted characters
├── assets/
│   ├── motd.txt                       # Message of the day
│   └── login-banner.txt               # ASCII art banner
└── .planning/
    └── architecture.md                # Full architecture walkthrough
```

## Conventions

- **Password hashing**: SHA-256 (PoC only — no salt. Replace with bcrypt/argon2 before production).
- **Persistence**: JSON filesystem. Accounts keyed by UUID, characters keyed by name.
- **Linkdead**: On disconnect, character stays in-world with `Linkdead` + `Player { connection: None }`. On reconnect, player skips MOTD and re-enters the world.
- **Output formatting**: Lives in `grim-client::formatter`. Protocol layer never formats — it just writes bytes.
- **Testing**: Each plugin has `#[cfg(test)] mod tests` inline. Tests use Bevy `App` + `update()`.
- **Directions**: Single-letter shortcuts (`n`/`e`/`s`/`w`/`u`/`d`) are checked first in the parser, so they always beat alphabetic command overlap.
- **`Exit` vs `Exits`**: The component is `Exits { exits: HashMap<Cardinal, Entity> }`. On a Room entity.
- **`Name` vs `GrimName`**: Import aliased from `grim` as `GrimName` in the binary and client to avoid collisions with Bevy's `Name`.
- **`rust-i18n` locale files**: Must live at `locales/<locale>.json` (e.g. `locales/en.json`). NEVER nest in subdirectories (`locales/en/auth.json`) — `rust-i18n` v4 won't find them and silently returns the key string. `set_locale!("en")` MUST be called in `ClientPlugin::build()` or the macro returns keys verbatim.

### Color Markup

Two markup formats are parsed by `grim::color::ansi()` and converted to ANSI
escape sequences. The conversion happens at the protocol layer in
`grim-protocol-telnet`'s `send_network_commands`, right before text is written
to the TCP socket. This ensures every byte leaving the server is already
ansi-encoded, regardless of which code path produced it.

Two markup formats supported:
- **16-color terminal codes** (`{code`): `{r`/`{R`, `{g`/`{G`, `{b`/`{B`, etc.
  with numeric (`{1`–`{9`) and symbol (`{!`, `{@`, etc.) aliases. See
  `grim::color` doc comment for the full table.
- **24-bit hex codes** (`@xRGB` / `@bRGB`): 3-digit hex, each nibble scaled by
  17 to 8-bit (e.g. `@xf00` → red, `@xfff` → white).
- **Escape**: `{{` → `{`, `@@` → `@`. Escaped codes pass through as literal
  text without color interpretation.
- Unknown codes pass through as literal text.

Assets (MOTD, login banner) also support color codes — put them directly in the
`.txt` file.

## Running

```bash
cargo run                        # start server on port 4000
telnet localhost 4000            # connect
```


## Workflow

All work follows a branch → PR → review → merge cycle.

0. **Branch pre-check** — At session start, check current branch. If it doesn't match the task,
   create a new branch from `main` first before any edits. Never modify unrelated work.
1. **Branch** from `main` — descriptive name, no convention beyond readable.
2. **Commit** incrementally — each commit is a coherent step.
3. **Push** the branch.
4. **Create a PR** — `gh pr create --fill --base main`. Pushing without opening a PR
   is unfinished work. Always do both.
5. **CI** runs checks: build, lint, test.
6. **Human review** — at least one set of eyes before merge.
7. **Squash merge** into `main`.

Pushing directly to `main` is reserved for trivial one-offs (docs, README tweaks). Everything else — any code change — goes through a PR.
## Security Notes (current PoC)

- SHA-256 for passwords — no salt, not slow.
- No rate limiting on login attempts.
- No crash recovery (crashed process loses in-flight connections, characters persist).
- No TLS — raw telnet, passwords in cleartext on the wire.

## Keeping These Files Honest

This file and `README.md` are the two entry points for anyone (or any agent) landing in this repo. When you ship a feature, change an architecture decision, or spot something stale:

- **Update this file** if the crate map, event flow, component list, or conventions changed.
- **Update README.md** if the status, roadmap, or getting-started steps changed.
- If you delete a crate, component, or event type, remove it from both files — dead entries mislead more than missing ones.

No separate "docs sprint". Make it part of the PR that introduces the change. A one-line addition to the roadmap or a modified event description in the table takes 30 seconds and saves the next person (including future-you) an hour of spelunking.

## Editing Discipline

Agent edits to this repo MUST follow these rules to avoid the session-wasting
pattern of stale-snapshot → broken edit → fix → re-fix:

1. **Re-ground after every edit.** Every `edit` call mints a fresh snapshot tag
   and renumbers lines. Take the next edit's line numbers from the edit response
   or a fresh `read` — never from memory.
2. **Verify structure before chaining.** After a multi-hunk or block edit, do a
   `read` of the affected function/module before issuing the next edit.
3. **Prefer rewrite over surgical patch when code is young.** Files in active
   development (<5 edits old) are faster to `write` from scratch than to fix up
   with staggered patches. Once stable, use surgical `edit`.
4. **Read the whole function before touching it.** A 2-line view of a larger
   function risks deleting content you didn't see. Elided ranges in `read`
   output mean "unseen" — expand the range.
5. **Test immediately after the last edit.** Don't chain 5 edits without a
   compile check. The build is cheap and catches structural damage fast.
6. **Range characters, not ASCII ranges, for non-contiguous sets.**
   `'k'..='w'` covers `klmnopqrstuvw`, not `krgybmcw`. Use
   `'k' | 'r' | 'g' | 'y' | 'b' | 'm' | 'c' | 'w'` for explicit sets.