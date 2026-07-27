# GRIM Engine

A modular MUD engine built on Bevy ECS. Everything in the world is an entity. Every interaction is an event. The engine is a library — a collection of Bevy plugins — and the running server is a binary that composes them.

For context: I come from a MUD called Waterdeep and am trying to recreate the general vibe, but with a more modern twist in Rust.

NOTE: This project leverages AI assistance to develop the software. All code is reviewed and curated by a human maintainer.

## Status

Currently functional: telnet login, account creation, character management, room movement, social channels, and basic world persistence. See `.planning/` for detailed architecture docs.

## Roadmap

### ✅ Done
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
├── Cargo.toml              # workspace root + binary entrypoint
├── crates/
│   ├── grim/               # engine library (components, events, plugins)
│   ├── grim-client/        # client state machine, parser, formatter
│   └── grim-protocol-telnet/  # TCP/telnet with tokio bridge
├── src/
│   ├── main.rs             # composes all plugins, starts server
│   └── seed.rs             # bootstraps the default world
└── .planning/              # architecture docs and plans
```