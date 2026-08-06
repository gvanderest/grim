# GRIM Engine

A modular MUD engine built on Bevy ECS. Everything in the world is an entity, every interaction is an event. The engine is a library — a collection of Bevy plugins — and the running server is a binary that composes them.

**Connect now, in your terminal:**

```sh
telnet grimtide.org 4000
```

For context: I come from a MUD called Waterdeep and am recreating the general vibe with a modern twist in Rust.

> NOTE: This project leverages AI assistance to develop the software. All code is reviewed and curated by a human maintainer.

## Getting started

```bash
cargo run
```

The workspace has one binary, `example-mud`, so a bare `cargo run` builds and runs it (or `cargo run -p example-mud`). It listens on port **4000**:

```bash
telnet localhost 4000
```

## Documentation

- **[docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md)** — authoritative target architecture, the gap to today (§8), and the forward-looking roadmap.
- **[CONTEXT.md](./CONTEXT.md)** — the glossary. Fixes the vocabulary (Session, Scene, Router, Actor, Attempt/Fact, Catalog, …) that crate, component, and event names must agree with.
- **[AGENTS.md](./AGENTS.md)** — how to work in this repo (for AI agents and humans alike), incl. the conventions and editing discipline.
- **Per-crate `README.md`** — each crate documents its own components, systems, and commands-with-handler-files, following [docs/README.template.md](./docs/README.template.md).

## Crates

Everything is a Bevy plugin unless noted. Dependencies point downward only; the `grim` facade re-exports every subsystem so a MUD author can depend on one crate. See each crate's README for its components, systems, and command→handler map.

### Foundation (pure libraries — no `App` state)

| Crate | What it holds |
|---|---|
| [`grim-color`](./crates/grim-color) | Colour markup (`{R`, `@xRGB`) → ANSI rendering, palette, escaping. No Bevy, no serde. |
| [`grim-text`](./crates/grim-text) | Text catalog: `tr`/`tr!`, colour-safe `%{var}` substitution, inlined defaults. Depends only on `grim-color`. |
| [`grim-command`](./crates/grim-command) | `CommandRegistry<C>` — generic exact-then-prefix resolution with explicit, reorderable priority. Bevy-only. |
| [`grim-engine-types`](./crates/grim-engine-types) | Shared primitives + the game-event vocabulary (`GrimId`, `Cardinal`, `Name`, `Description`, `RoomLocation`, `Gender`, `Command`, engine `Message` types). Slated to become `grim-core`. |

### Transport

| Crate | What it holds |
|---|---|
| [`grim-networking`](./crates/grim-networking) | `Connection` component + wire events (`ConnectionInput`/`Output`, `Established`/`Closed`, `DisconnectRequest`). Transport-agnostic. |
| [`grim-networking-telnet`](./crates/grim-networking-telnet) | `TelnetPlugin`: TCP server, IAC negotiation, tokio↔Bevy bridge, ANSI on the wire, copyover hot-restart. |

### Session

| Crate | What it holds |
|---|---|
| [`grim-scene`](./crates/grim-scene) | `ScenePlugin`: session lifecycle + login/creation state machine, input parsing, output formatting. Owns the `CommandRegistry` resource. |

### World & beings

| Crate | What it holds |
|---|---|
| [`grim-world`](./crates/grim-world) | `WorldPlugin` (areas/rooms/exits topology + race/class registries) and `ShutdownPlugin` (countdown + SIGTERM). The being-free *stage*. |
| [`grim-actor`](./crates/grim-actor) | `ActorPlugin`: the beings (`Character`/`Player`/`InRoom`/…) and the verbs that read them (`look`/`move`/`goto`/`quit`/`title`/`shutdown`). Depends on `grim-world`, never the reverse. |
| [`grim-channel`](./crates/grim-channel) | `ChannelPlugin`: social channels — say (room), yell (area), ooc (global). |

### Persistence & composition

| Crate | What it holds |
|---|---|
| [`grim-persistence`](./crates/grim-persistence) | `PersistencePlugin`: load accounts/characters from disk, save on disconnect. |
| [`grim`](./crates/grim) | Facade: depends on and re-exports every subsystem; offers `GrimDefaultPlugins` / `GrimHeadlessPlugins`. No logic of its own. |
| [`example-mud`](./crates/example-mud) | The runnable binary: `GrimDefaultPlugins` + a hardcoded world seed. |

## Architecture in one breath

Event-passing layers: no layer calls another's functions or reads its components — communication is exclusively through Bevy `Message` types. Transport frames bytes, the session turns input into an `EngineCommand`, gameplay plugins act on their command variant and emit world events, and the session formats those back to the connection. Colour markup is rendered to ANSI only at the transport edge, just before bytes hit the socket.

> ⚠️ "Client" is a retired name — it conflated the session state machine, wire framing, and the user's terminal. The session crate is `grim-scene`. See [CONTEXT.md](./CONTEXT.md).

**Status:** functional — telnet login, account creation, character management (gender/race/class/level), room movement, social channels, linkdead survival, and JSON world persistence. Security is proof-of-concept (SHA-256 password hashing, no TLS, no rate limiting) — not production-hardened.
