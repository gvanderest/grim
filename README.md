# GRIM Engine

A modular MUD engine built on Bevy ECS. Everything in the world is an entity. Every interaction is an event. The engine is a library — a collection of Bevy plugins — and the running server is a binary that composes them.

For context: I come from a MUD called Waterdeep and am trying to recreate the general vibe, but with a more modern twist in Rust.

NOTE: This project leverages AI assistance to develop the software. All code is reviewed and curated by a human maintainer.

## Status

Currently functional: telnet login, account creation, character management, room movement, social channels, and basic world persistence. See `.planning/` for detailed architecture docs.

## Roadmap
### ✅ Done
- [x] Color markup system: 16-color terminal codes + 24-bit hex codes → ANSI
- [x] i18n via rust-i18n: locale strings in locales/en.json
- [x] Protocol-layer newline conversion (\n → \r\n)
- [x] Workspace structure: `grim` library + `grim-client` + `grim-protocol-telnet` crates
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
- [ ] Finger command
- [ ] Chat channels with audience filters (global/area/room × clan/party/etc.)
- [ ] Proper telnet client negotiation handling

### 📋 Planned
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
# In another terminal:
telnet localhost 4000
```

## Project Structure

```
├── Cargo.toml              # Workspace root + binary
├── locales/
│   └── en.json              # i18n string resources
├── crates/
│   ├── grim/               # Engine library: components, events, plugins
│   │   └── src/
│   │       ├── lib.rs       # Module exports
│   │       ├── color.rs     # Color markup → ANSI escape conversion
│   │       ├── components.rs
│   │       ├── events.rs
│   │       ├── cardinal.rs
│   │       ├── validation.rs
│   │       ├── prelude.rs
│   │       └── plugins/
│   ├── grim-client/        # Client state machine, parser, formatter
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── parser.rs
│   │       └── formatter.rs
│   └── grim-protocol-telnet/  # TCP/telnet with tokio bridge
│       └── src/
│           └── lib.rs
├── assets/
│   ├── motd.txt            # Message of the day
│   └── login-banner.txt    # ASCII art login banner
├── data/                   # Persisted accounts/characters
└── .planning/              # Architecture docs and plans
```

## Architecture

4 event-passing layers, no layer calls another's functions or reads its components. Communication is exclusively through Bevy `Message` types.

| Layer | Crate | Responsibility | Output |
|---|---|---|---|
| Protocol | `grim-protocol-telnet` | Raw TCP, telnet IAC negotiation, tokio bridge | `ClientInput`, `ConnectionEstablished` |
| Client | `grim-client` | Session state machine, command parser, output formatter | `EngineCommand` |
| Engine | `grim` (plugins/) | ECS systems: rooms, movement, social channels, persistence | engine event → client formatting |
| Persistence | `grim::plugins::persistence` | Load accounts/characters from disk, save on disconnect | — |

## Color Markup

All text containing color markup is converted to ANSI escape sequences at the protocol layer in `send_network_commands`, right before bytes go to the TCP socket. Use `{` and `@` codes anywhere in output text.

### 16-color codes

| Input       | Color                    | ANSI |
|-------------|--------------------------|------|
| `{x` / `{9` | Reset                    | 0    |
| `{k` / `{1` | Black                    | 30   |
| `{r` / `{1` | Red                      | 31   |
| `{g` / `{2` | Green                    | 32   |
| `{y` / `{3` | Yellow                   | 33   |
| `{b` / `{4` | Blue                     | 34   |
| `{m` / `{5` | Magenta                  | 35   |
| `{c` / `{6` | Cyan                     | 36   |
| `{w` / `{7` | White                    | 37   |
| `{K` / `{8` / `{*` | Bright Black (Grey) | 90  |
| `{R` / `{!` | Bright Red               | 91   |
| `{G` / `{@` | Bright Green             | 92   |
| `{Y` / `{#` | Bright Yellow            | 93   |
| `{B`        | Bright Blue              | 94   |
| `{M` / `{%` | Bright Magenta           | 95   |
| `{C` / `{^` | Bright Cyan             | 96   |
| `{W` / `{&` | Bright White             | 97   |

### 24-bit hex codes

| Code     | Effect                         |
|----------|--------------------------------|
| `@r`     | Reset                          |
| `@xRGB`  | Foreground (3 hex digits, scaled nibble×17 to 8-bit) |
| `@bRGB`  | Background (same scaling)       |

### Escaping

| Input | Output |
|-------|--------|
| `{{`  | `{`    |
| `@@`  | `@`    |

Unknown codes (e.g. `{z`, `{-`, `@q`) pass through as literal text.

## Events

| Event | Direction | Trigger |
|---|---|---|
| `ConnectionEstablished` | Protocol→Client | Telnet handshake done |
| `ClientInput` | Protocol→Client | Raw line from socket |
| `ConnectionClosed` | Protocol→All | Socket dropped |
| `ClientOutput` | Any→Protocol | Text to send (ansi converted here) + optional ECHO flag |
| `DisconnectRequest` | Client→Protocol | Clean socket close |
| `EngineCommand` | Client→Engine | Parsed command |
| `LookRoom` | Engine→Client | Format room description |
| `LookEntity` | Engine→Client | Format entity description |
| `SayEvent`/`YellEvent`/`OocEvent` | Engine→Client | Social channel broadcasts |
| `MoveEvent` | Engine→Client | Room transition broadcast |
| `InfoMessage` | Engine→Client | Direct text to one character |
| `LoginAnnounce`/`LogoutAnnounce`/`LinkdeadAnnounce` | Engine→Client | Global player announcements |

## Conventions

- **Newlines**: Use `\n` in all code. Protocol layer converts `\n` → `\r\n` before writing to TCP.
- **Password hashing**: SHA-256 (PoC — no salt. Use bcrypt/argon2 before production).
- **Persistence**: JSON filesystem. Accounts keyed by UUID, characters keyed by name.
- **Linkdead**: Character stays in-world with `Linkdead` + `Player { connection: None }` on disconnect. On reconnect, skips MOTD and re-enters.
- **Output formatting**: Lives in `grim-client::formatter`. Protocol layer never formats — it only writes bytes.
- **Directions**: Single-letter shortcuts (`n`/`e`/`s`/`w`/`u`/`d`) beat alphabetic command overlap in the parser.
- **i18n + color strings**: Use `tr!()` macro (from `grim::color`) for locale strings containing color codes. Reads `locales/en.json`, converts `{X}` 16-color codes to `@xRGB` format (avoids i18n brace conflicts), then substitutes `%{var}` placeholders. Plain strings without colors still use `t!()` from rust-i18n.
- **Security**: No TLS, no rate limiting, no crash recovery. SHA-256 only.