# GRIM MUD Architecture

## The Big Picture

```
┌─────────────────────────────────────────────────────────────────────┐
│                    USER MUD BINARY                                 │
│  (cargo new my-mud / workspace root)                              │
│                                                                     │
│  [dependencies]                                                     │
│  grim = "0.1"                                                       │
│  grim-client = "0.1"                                                │
│  grim-protocol-telnet = "0.1"                                       │
│  my-custom-plugin = { path = "./plugins/my-custom" }               │
│                                                                     │
│  fn main() {                                                       │
│      let mut app = App::new();                                     │
│                                                                     │
│      // Install plugins (composition)                              │
│      app.add_plugins(grim::plugins::WorldPlugin);                  │
│      app.add_plugins(grim::plugins::SocialPlugin);                 │
│      app.add_plugins(grim::plugins::PersistencePlugin);            │
│      app.add_plugins(grim_client::ClientPlugin);                   │
│      app.add_plugins(grim_protocol_telnet::TelnetPlugin::new(4000));│
│                                                                     │
│      // Seed the world                                             │
│      app.add_systems(Startup, seed::seed_world);                   │
│                                                                     │
│      app.run();                                                    │
│  }                                                                  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Key Concepts

### 1. `grim-engine-types` — Core Types

**Purpose:** Pure types for MUD components, events, commands, and cardinal directions.

**What it provides:**
- Bevy ECS types (Components, Events, Messages)
- MUD-specific types (Cardinal, MudCommand)
- Command registry (CommandRegistry)

**Key principle:** These are just types. You can use them directly or wrap them.

### 2. `grim` — Compatibility Layer

**Purpose:** Thin wrapper that re-exports `grim-engine-types` for backward compatibility.

**Key principle:** This is a transitional crate. Eventually, plugins will import directly from `grim-engine-types`.

### 3. Network Layer — `grim-core-networking` (future)

**Purpose:** Normalized interface for network protocols.

**Structure:**
```
Protocol (Telnet, SSH, WebSocket) → grim-core-networking → Client
```

**What grim-core-networking defines:**
- Server/Connection trait (what a "server" looks like to GRIM)
- ProtocolPlugin trait (how protocols register against the network layer)
- Standardized incoming/outgoing message formats

**Protocol-specific handling:**
- Telnet: ANSI color support, IAC negotiation
- SSH: Raw ANSI pass-through
- WebSocket: Protocol-agnostic (client handles HTML/ANSI/etc.)

### 4. Client Layer — `grim-client`

**Purpose:** Client session state machine, command parser, output formatter.

**What it does:**
- Parses incoming protocol messages into `MudCommand`
- Handles connection state (Disconnected → Connected → Authenticated → InGame)
- Formats outgoing game events for the protocol

**What it does NOT do:**
- Protocol-specific formatting (ANSI vs HTML) - that's the protocol's job
- Game logic - that's in grim-core-* plugins

### 5. Core Plugins — `grim::plugins/*`

**Purpose:** Default MUD functionality provided by GRIM.

| Module | Purpose |
|--------|---------|
| `world.rs` | Basic world commands (look, move, quit) |
| `social.rs` | Social commands (say, yell, ooc) |
| `persistence.rs` | Account/character save/load |

**Key principle:** These are just Bevy plugins. Users can write their own.

---

## Event Flow

```
┌─────────────┐     ┌──────────────┐     ┌──────────┐     ┌──────────┐
│  Protocol   │────▶│   Client     │────▶│   Game   │────▶│  Client  │
│  (Telnet/   │     │ (Command     │     │ (Systems │     │ (Format   │
│   SSH/WS)   │     │  Parsing)    │     │  + Plugins│    │  Output) │
└─────────────┘     └──────────────┘     └──────────┘     └──────────┘
        │                    │                  │                  │
        ▼                    ▼                  ▼                  ▼
   ClientInput           EngineCommand      Game Events        ClientOutput
   ConnectionEstablished  MudCommand        LookRoom           (Protocol-)
   ConnectionClosed       CommandRegistry   SayEvent          specific)
```

---

## Key Principles

1. **`grim-engine-types` has all core types** — Components, Events, Commands, Cardinal
2. **Network is a plugin** — `grim-core-networking` provides the interface, protocols register against it
3. **Client handles session** — State machine, command parsing, output formatting
4. **Game is Bevy ECS** — Core plugins are just Bevy systems on top of the ECS
5. **Protocol-specific** — Each protocol handles its own formatting (ANSI, HTML, etc.)
6. **`MudCommand` is the normalized interface** — Everything flows through this

---

## Current State

### Implemented
- `grim-engine-types` - Core types extracted
- `grim` - Compatibility re-export layer
- `grim-client` - Client session state machine
- `grim-protocol-telnet` - Telnet protocol implementation
- `grim::plugins` - World, Social, Persistence plugins

### TODO
- `grim-core-networking` - Network interface plugin
- Move core plugins to separate crates
- Update docs/ARCHITECTURE.md to reflect new structure