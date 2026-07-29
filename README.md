# GRIM Engine

A modular MUD engine built on Bevy ECS. Everything in the world is an entity. Every interaction is an event. The engine is a library — a collection of Bevy plugins — and the running server is a binary that composes them.

For context: I come from a MUD called Waterdeep and am trying to recreate the general vibe, but with a more modern twist in Rust.

NOTE: This project leverages AI assistance to develop the software. All code is reviewed and curated by a human maintainer.

## Documentation

- **[docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md)** — authoritative target architecture and the gap between it and today.
- **[CONTEXT.md](./CONTEXT.md)** — the glossary. Fixes the vocabulary (Session, Scene, Router, Attempt/Fact, Catalog, …) that crate, component, and event names must agree with.
- **[AGENTS.md](./AGENTS.md)** — how to work in this repo (for AI agents and humans alike).

## Status

Currently functional: telnet login, account creation, character management, room movement, social channels, and basic world persistence.

## Roadmap
### ✅ Done
- [x] Colour markup system: 16-colour terminal codes + 24-bit hex codes → ANSI (`grim-color`)
- [x] Text catalog: `tr`/`tr!` with author-facing defaults, colour-safe `%{var}` substitution (`grim-text`)
- [x] Command resolution: exact-then-prefix `CommandRegistry` with explicit, reorderable priority (`grim-command`)
- [x] Protocol-layer newline conversion (`\n` → `\r\n`)
- [x] Workspace decomposition: `grim` facade + `grim-engine-types` + `grim-color`/`grim-text`/`grim-command` + `grim-client` + `grim-protocol-telnet` + `example-mud`
- [x] Telnet server with IAC negotiation and password masking
- [x] Account creation and login (email only)
- [x] Character creation, selection, and persistence
- [x] MOTD gate before entering the world
- [x] Room movement (north/east/south/west/up/down)
- [x] Social channels: say (room), yell (area), ooc (global)
- [x] Look, who, where, commands, quit
- [x] Linkdead characters: crash keeps character in-world, reconnect skips MOTD
- [x] Persistence: save on quit to `data/accounts/*.json` + `data/characters/*.json`
- [x] Server-side logging via `bevy_log`
- [x] Prompt (`> `) after every command response

### 🚧 In Progress
- [ ] Crate decomposition steps 4–9 (transport/scene/session rework — see ARCHITECTURE.md §8)
- [ ] Finger command
- [ ] Chat channels with audience filters (global/area/room × clan/party/etc.)
- [ ] Proper telnet client negotiation handling

### 📋 Planned
- [ ] Scene stack + Router as first-class subsystems (retire the `Client` state machine)
- [ ] Externalized text catalog (`strings/<locale>/`, `templates/<locale>/`) — defaults are inlined today
- [ ] Websocket server support
- [ ] Combat system
- [ ] Items, equipment, and inventory
- [ ] Drinking potions, eating food and pills
- [ ] Experience, levels, and skills
- [ ] Race/class/level type systems
- [ ] Areas and rooms from file definitions (not hardcoded seed)
- [ ] Crash recovery without losing connections
- [ ] Copyover / builder port
- [ ] Persistent storage strategy (SQLite? Filesystem?)

## Getting Started

```bash
cargo run
```

The workspace has one binary, `example-mud`, so a bare `cargo run` builds and runs it (or `cargo run -p example-mud`). It listens on port **4000**:

```bash
telnet localhost 4000
```

## Project Structure

```
├── Cargo.toml                    # Workspace root (no binary here)
├── assets/
│   ├── motd.txt                  # Message of the day
│   └── login-banner.txt          # ASCII art login banner
├── crates/
│   ├── grim/                     # Engine facade: re-exports types + tr + CommandRegistry, owns plugins
│   │   └── src/
│   │       ├── lib.rs
│   │       └── plugins/          # world, social, persistence
│   ├── grim-engine-types/        # Wire/game events (Message types), components, cardinal, validation
│   ├── grim-color/               # Colour markup → ANSI (no Bevy, no serde)
│   ├── grim-text/                # Text catalog: tr/tr!, inlined defaults (depends only on grim-color)
│   ├── grim-command/             # CommandRegistry<C>: exact-then-prefix resolution (Bevy-only)
│   ├── grim-client/              # Session state machine, command parser, output formatter
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── parser.rs
│   │       └── formatter.rs
│   ├── grim-protocol-telnet/     # TCP/telnet with tokio bridge
│   └── example-mud/              # Binary: composes plugins, seeds the world
│       └── src/
│           ├── main.rs
│           └── seed.rs
├── docs/
│   └── ARCHITECTURE.md
├── CONTEXT.md
├── AGENTS.md
└── data/                         # Persisted accounts/characters (gitignored)
```

## Architecture

Event-passing layers, no layer calls another's functions or reads its components. Communication is exclusively through Bevy `Message` types.

| Layer | Crate | Responsibility | Output |
|---|---|---|---|
| Protocol | `grim-protocol-telnet` | Raw TCP, telnet IAC negotiation, tokio bridge, colour → ANSI on the wire | `ClientInput`, `ConnectionEstablished` |
| Client | `grim-client` | Session state machine, command parser, output formatter | `EngineCommand` |
| Engine | `grim` (`plugins/`) | ECS systems: rooms, movement, social channels, persistence | engine event → client formatting |
| Persistence | `grim::plugins::persistence` | Load accounts/characters from disk, save on disconnect | — |

Shared vocabulary lives in `grim-engine-types` (events, components); pure subsystems (`grim-color`, `grim-text`, `grim-command`) carry no `App` state and stay plain libraries. The `grim` crate re-exports all of them so downstream code depends on one facade.

⚠️ "Client" is a retired name in the target architecture — it conflated the session state machine, wire framing, and the user's terminal. See [CONTEXT.md](./CONTEXT.md).

## Colour Markup

All text containing colour markup is converted to ANSI escape sequences at the protocol layer, right before bytes go to the TCP socket. Rendering lives in `grim-color` (`ansi`, `convert_16color`, `escape_codes`), re-exported at `grim::color::*`. Use `{` and `@` codes anywhere in output text.

### 16-colour codes

| Input       | Colour                    | ANSI |
|-------------|---------------------------|------|
| `{x` / `{9` | Reset                     | 0    |
| `{k`        | Black                     | 30   |
| `{r` / `{1` | Red                       | 31   |
| `{g` / `{2` | Green                     | 32   |
| `{y` / `{3` | Yellow                    | 33   |
| `{b` / `{4` | Blue                      | 34   |
| `{m` / `{5` | Magenta                   | 35   |
| `{c` / `{6` | Cyan                      | 36   |
| `{w` / `{7` | White                     | 37   |
| `{K` / `{8` / `{*` | Bright Black (Grey)| 90   |
| `{R` / `{!` | Bright Red                | 91   |
| `{G` / `{@` | Bright Green              | 92   |
| `{Y` / `{#` | Bright Yellow             | 93   |
| `{B`        | Bright Blue               | 94   |
| `{M` / `{%` | Bright Magenta            | 95   |
| `{C` / `{^` | Bright Cyan               | 96   |
| `{W` / `{&` | Bright White              | 97   |

(`{k` black has no digit alias — the digit `1` is red.)

### 24-bit hex codes

| Code     | Effect                                               |
|----------|------------------------------------------------------|
| `@r`     | Reset                                                |
| `@xRGB`  | Foreground (3 hex digits, scaled nibble×17 to 8-bit) |
| `@bRGB`  | Background (same scaling)                             |

### Escaping

| Input | Output |
|-------|--------|
| `{{`  | `{`    |
| `@@`  | `@`    |

Unknown codes (e.g. `{z`, `{-`, `@q`) pass through as literal text.

## Events

All inter-layer events are Bevy `Message` types (`#[derive(Message)]`).

| Event | Direction | Trigger |
|---|---|---|
| `ConnectionEstablished` | Protocol→Client | Telnet handshake done |
| `ClientInput` | Protocol→Client | Raw line from socket |
| `ConnectionClosed` | Protocol→All | Socket dropped |
| `ClientOutput` | Any→Protocol | Text to send (ANSI converted here) + optional ECHO flag |
| `DisconnectRequest` | Client→Protocol | Clean socket close |
| `EngineCommand` | Client→Engine | Parsed `Command` |
| `LookRoom` | Engine→Client | Format room description |
| `LookEntity` | Engine→Client | Format entity description |
| `SayEvent`/`YellEvent`/`OocEvent` | Engine→Client | Social channel broadcasts |
| `MoveEvent` | Engine→Client | Room transition broadcast |
| `InfoMessage` | Engine→Client | Direct text to one character |
| `LoginAnnounce`/`LogoutAnnounce`/`LinkdeadAnnounce` | Engine→Client | Global player announcements |

## Conventions

- **Newlines**: Use `\n` in all code. Protocol layer converts `\n` → `\r\n` before writing to TCP.
- **Text catalog**: Use `grim::tr!("key", var = value)` (or `grim::tr(key, args)`) for author-facing strings — this is `grim-text`. It converts `{X` colour codes to `@xRGB`, then substitutes `%{var}` placeholders, escaping each value so it cannot inject colour. Defaults are inlined in `grim-text`; `rust-i18n`/`t!`/`locales/en.json` are gone.
- **Command resolution**: `grim::CommandRegistry<Command>` (from `grim-command`). Registered by name + `fn(&str) -> Option<Command>` factory. Case-insensitive, exact name first then highest-priority prefix. Priority is explicit and reorderable (`prioritize`/`deprioritize`) — single-letter directions (`n`/`e`/…) win by being registered last. Still held in a `OnceLock`, not yet a live resource — see ARCHITECTURE.md §8.
- **Password hashing**: SHA-256 (PoC — no salt. Use bcrypt/argon2 before production).
- **Persistence**: JSON filesystem. Accounts keyed by UUID, characters keyed by name.
- **Linkdead**: Character stays in-world with `Linkdead` + `Player { connection: None }` on disconnect. On reconnect, skips MOTD and re-enters.
- **Output formatting**: Lives in `grim-client::formatter`. Protocol layer never formats — it only writes bytes.
- **Directions**: Single-letter shortcuts (`n`/`e`/`s`/`w`/`u`/`d`) beat alphabetic command overlap in the parser.
- **Fail closed**: Ownership/visibility checks must show nothing when their gating lookup fails — never fall through to unfiltered data. See AGENTS.md.
- **Security**: No TLS, no rate limiting, no crash recovery. SHA-256 only.
