# GRIM MUD Architecture

## The Big Picture

```
┌─────────────────────────────────────────────────────────────────────┐
│                    USER MUD BINARY                                 │
│  (cargo new my-mud / workspace root)                              │
│                                                                     │
│  [dependencies]                                                     │
│  grim-engine = "0.1"                                                │
│  grim-core-world = "0.1"                                            │
│  grim-core-social = "0.1"                                           │
│  my-custom-plugin = { path = "./plugins/my-custom" }               │
│                                                                     │
│  fn main() {                                                       │
│      let mut engine = GrimEngine::new();                           │
│                                                                     │
│      // Install plugins (composition)                              │
│      engine.add_plugin(grim_core_world::WorldPlugin::default());   │
│      engine.add_plugin(grim_core_social::SocialPlugin::default()); │
│      engine.add_plugin(MyCustomPlugin::default());                 │
│                                                                     │
│      // Add network protocols (can run multiple in parallel)       │
│      engine.add_network_protocol(Telnet { port: 4000 });          │
│      engine.add_network_protocol(Telnet { port: 4200 });          │
│      engine.add_network_protocol(SSH { port: 22 });               │
│                                                                     │
│      engine.run();                                                 │
│  }                                                                  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Key Concepts

### 1. `grim-engine` — The Engine

**Purpose:** Bevy ECS wrapper + binding point for plugins.

**What it provides:**
- ECS wrapper with MUD-specific types
- Plugin registry (`add_plugin()`)
- Plugin trait (`grim_engine::Plugin`)
- Network protocol registry (`add_network_protocol()`)
- Storage driver registry

**What it DOES NOT do:**
- Implement game features
- Parse commands
- Store data

**Key principle:** THIS IS THE ONLY THING THAT'S NOT A PLUGIN.

### 1a. Plugin Configuration

**Purpose:** Allow plugins to be customized without breaking encapsulation.

**Pattern:** Builder-style configuration on plugin default:

```rust
// WorldPlugin with default config
engine.add_plugin(WorldPlugin::default());

// WorldPlugin with custom config
engine.add_plugin(
    WorldPlugin::default()
        .with_config(WorldPluginConfig {
            after_command_delay_ms: 250,  // Override 500ms default
        })
);

// Network protocol with custom config
engine.add_network_protocol(
    Telnet::default()
        .with_port(4000)
        .with_max_connections(100)
);
```

**Key principles:**
- Plugins own their configuration types
- `default()` provides sensible defaults
- `.with_*()` methods are chainable
- Config can be changed without recompiling the plugin

### 2. Core Plugins — `grim-core-*`

**Purpose:** Default MUD functionality provided by GRIM.

**What they are:**
- **Optional** plugins that implement `grim_engine::Plugin`
- Can be swapped out or omitted
- "Batteries included" but not required

**All crates are sibling crates at the same level:**

| Crate | Purpose |
|-------|---------|
| `grim-core-world` | Basic world commands (look, move, quit) |
| `grim-core-social` | Social commands (say, yell, ooc) |
| `grim-core-persistence` | Account/character save/load |
| `grim-core-combat` | Combat system (future) |
| `grim-core-networking` | Network protocol abstraction (telnet/ws/ssh) |

**Key principle:** These are just plugins like any other. Users can write their own implementations.

### 3. User MUD Binary — "User Land"

**Purpose:** Compose a MUD by selecting plugins.

**What users do:**
1. Create a new Rust project (cargo new my-mud) or use a workspace
2. Add `grim-engine` + desired crates to Cargo.toml
3. Implement custom plugins (optional)
4. Configure engine (network protocols, storage, etc.)
5. Run!

**Example user project structure:**
```
my-mud/
├── Cargo.toml
├── src/
│   └── main.rs
└── plugins/
    └── my-custom/
        ├── Cargo.toml
        └── src/
            └── lib.rs
```

### 4. Custom Plugins

**Purpose:** User or community-written plugins.

**Naming:** Plugin authors choose their own crate names. Examples:
- `some-custom-world`
- `bobs-crafting`
- `my-mud-combat`

**Can be:**
- Local path dependencies (`path = "./plugins/my-custom"`)
- Published to crates.io (`crate-name = "0.1"`)

---

## Crate Layout

All crates are sibling crates at the same level under `crates/`:

```
crates/
├── grim-engine/               # THE ENGINE (only non-plugin)
│   └── src/
│       ├── lib.rs             # Plugin trait, Engine builder
│       ├── ecs/               # ECS primitives
│       │   ├── components.rs
│       │   ├── events.rs
│       │   └── commands.rs
│       └── protocol/          # Protocol shapes
│           ├── mod.rs
│           ├── shapes.rs
│           └── telnet.rs
├── grim-client/               # Client session layer (optional)
│   └── src/
│       ├── lib.rs
│       ├── parser.rs
│       └── formatter.rs
├── grim-protocol-telnet/      # Telnet protocol (plugin)
│   └── src/
│       └── lib.rs
├── grim-core-world/           # Core plugin (look, move, quit)
├── grim-core-social/          # Core plugin (say, yell, ooc)
├── grim-core-persistence/     # Core plugin (save/load)
├── grim-core-combat/          # Core plugin (combat, future)
├── grim-core-networking/      # Network protocol abstraction
└── my-custom-world/           # Example community plugin
```

---

## Key Principles

1. **`grim-engine` is the only non-plugin** — It's the ECS wrapper + binding point
2. **Everything else is a plugin** — Even `grim-core-*` and `grim-client`
3. **Composition over configuration** — Users pick plugins in `main.rs`
4. **Users can write their own plugins** — Any name they choose
5. **Network/storage are pluggable** — Multiple protocols can run in parallel
6. **Plugins are configurable** — Builder pattern via `.with_*()` methods

---

## Network Protocol Example

Users can run multiple protocols simultaneously:

```rust
use grim_engine::{GrimEngine, Plugin};
use grim_engine::protocol::Telnet;
use grim_engine::protocol::SSH;

fn main() {
    let mut engine = GrimEngine::new();
    
    engine.add_plugin(grim_core_world::WorldPlugin::default());
    
    // Run Telnet on ports 4000 and 4200
    engine.add_network_protocol(Telnet { port: 4000 });
    engine.add_network_protocol(Telnet { port: 4200 });
    
    // Run SSH on port 22
    engine.add_network_protocol(SSH { port: 22 });
    
    engine.run();
}
```

---

## Current State vs Target State

### Current State
- `grim` crate contains mixed concerns (components, events, plugins, commands)
- Commands hardcoded in `parser.rs` via `OnceLock`
- Plugins are thin wrappers with no configuration hooks
- No clear separation between engine and game logic

### Target State
- `grim-engine` is clean ECS wrapper + plugin trait
- `grim-core-*` are separate crates for core functionality
- Users compose their MUD by installing plugins
- Multiple network protocols can run in parallel
- Plugins support configuration via builder pattern

---

## Migration Path

### Phase 1: Extract `grim-engine`
- Create `crates/grim-engine/`
- Move ECS primitives (`components`, `events`, `commands`)
- Define `Plugin` trait with `.with_*()` methods
- Keep `grim` as compatibility layer

### Phase 2: Extract Core Plugins
- `grim-core-world` from `plugins/world.rs`
- `grim-core-social` from `plugins/social.rs`
- `grim-core-persistence` from `plugins/persistence.rs`
- `grim-core-networking` for protocol abstraction
- Update `grim-engine` to use new plugin trait

### Phase 3: Update Binary
- Migrate `src/main.rs` to use `GrimEngine::new()`
- Install plugins via `add_plugin()`
- Add protocols via `add_network_protocol()`

---

## Developing New Features

**When adding a new feature, ask:**

1. **Does this already fit into the architecture?**
   - If yes → Implement as a plugin
   - If no → Proceed to question 2

2. **Does this require a change to the architecture?**
   - If yes → Document the change in `docs/ARCHITECTURE.md`
   - If no → Proceed to question 3

3. **Does this add new functionality which needs API-level discussion?**
   - If yes → Open an issue to discuss design
   - If no → Implement

**Reference:** `docs/ARCHITECTURE.md` for current architecture decisions.