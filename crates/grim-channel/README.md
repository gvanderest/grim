# `grim-channel`
> Player-speech channels: data-driven configuration for `say`, `yell`, `ooc`, `tell`/`whisper`, `reply`, and admin `gecho`.

**Role:** vertical — player speech / communication channels
**Depends on:** `grim-core`, `grim-world`, `grim-actor`, `grim-text`

## Components
| Component | File | Purpose |
|---|---|---|
| `LastWhisperFrom(Entity)` | `src/whisper.rs` | Records the last player who whispered this entity, so `reply` can target them. |

## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|
| `handle_channel` | `Update` | `src/handler.rs` | **Data-driven** unified channel handler - reads `Command::Channel`, emits `ChannelMessage` based on channel config. |
| `handle_gecho` | `Update` | `src/commands/gecho.rs` | Reads `Command::Gecho`, emits `GlobalEcho`. Re-checks admin (defense in depth). |
| `handle_tell` | `Update` | `src/commands/tell.rs` | Reads `Command::Tell`, fuzzy-matches the target player, delivers a private whisper. |
| `handle_reply` | `Update` | `src/commands/reply.rs` | Reads `Command::Reply`, whispers the entity's `LastWhisperFrom`. |

## Commands
Player-facing verbs and where to find their handlers.
| Command | Handler | Summary |
|---|---|---|
| `say <text>` | Data-driven (via `Command::Channel`) | Broadcast to the current room; configured via `ChannelRegistry`. |
| `yell <text>` | Data-driven (via `Command::Channel`) | Broadcast to every room in the actor's area; configured via `ChannelRegistry`. |
| `ooc <text>` | Data-driven (via `Command::Channel`) | Out-of-character global chat; configured via `ChannelRegistry`. |
| `tell <target> <text>` | `handle_tell` (`src/commands/tell.rs`) | Private message to one player (case-insensitive name prefix; `self` targets sender). |
| `whisper <target> <text>` | `handle_tell` (`src/commands/tell.rs`) | Alias for `tell` (parsed in `grim-scene`). |
| `reply <text>` | `handle_reply` (`src/commands/reply.rs`) | Whisper the last player who whispered you (`LastWhisperFrom`). |
| `gecho <text>` | `handle_gecho` (`src/commands/gecho.rs`) | Admin-only world-wide echo. Gated at dispatch in `grim-scene`; re-checked here. |

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `ChannelRegistry` | Resource | `src/registry.rs` | Stores registered channel configurations (name, scope, eligibility). |
| `ChannelMessage` | Message | `src/message.rs` | **Data-driven** unified channel event - one shared event with data-driven scope/resolution. |
| `GlobalEcho` | Message (consumed) | `src/commands/gecho.rs` |
| `EngineCommand` | Message (input) | `src/handler.rs`, `src/commands/*.rs` |
| `InfoMessage` | Message (output echo) | `src/handler.rs`, `src/commands/*.rs` |

## Configuration
Channels are **data**, not code. Register channels via `ChannelRegistry`:

```rust
use grim_channel::{Channel, Scope, Identify, SpeakEligibility, ListenEligibility};

app.add_plugins(ChannelPlugin);
// ChannelPlugin initializes ChannelRegistry with say/yell/ooc by default

// Add a custom channel
app.world_mut().resource_mut::<ChannelRegistry>().add_channel(Channel {
    name: "gossip".to_string(),
    scope: Scope::Global,
    identify: Identify::Always,
    toggleable: true,
    speak: SpeakEligibility::Authenticated,
    listen: ListenEligibility::Authenticated,
    key: "channel.gossip".to_string(),
});
```

**Axes:**
| Axis | Values | Note |
|------|--------|------|
| `scope` | `Room`, `Area`, `Global` | Earshot only — see ARCHITECTURE.md §7 |
| `identify` | `Perceived` / `Always` | `Perceived` uses in-world sound; `Always` is OOC |
| `toggleable` | bool | Per-player subscription state, owned by `grim-persistence` |
| `speak` | predicate | Who may send |
| `listen` | predicate | Who may receive — separate from `speak` |
| `key` | Catalog prefix | Resolves `.first_party` / `.third_party` |

## Default Channels
The `ChannelPlugin` automatically registers these channels:

| Name | Scope | Identify | Toggleable | Speak | Listen | Catalog Key |
|------|-------|----------|------------|-------|--------|-------------|
| `say` | Room | Perceived | No | All | All | `channel.say` |
| `yell` | Area | Perceived | No | All | InArea | `channel.yell` |
| `ooc` | Global | Always | Yes | All | All | `channel.ooc` |

## Notes
- **Data-driven channels (ARCHITECTURE.md §7):** The `ChannelMessage` event replaces the distinct `Say/Yell/Ooc` events. Channel behavior (scope, audience, formatting) is determined by the channel configuration stored in `ChannelRegistry`.
- The parser (`grim-scene/src/parser.rs`) emits `Command::Channel { channel: "say", text }` for `say`, `yell`, `ooc` commands.
- The handler (`handle_channel`) looks up the channel config from `ChannelRegistry` and emits `ChannelMessage`.
- The renderer (`grim-scene/src/output.rs`) reads `ChannelMessage`, uses the channel's `key` to look up the catalog template, and broadcasts to the scope-resolved audience.
- `gecho` admin gating is enforced at dispatch (`grim-scene`) and re-verified in `handle_gecho` — fails closed (silent) for non-admins.
- `tell`/`whisper` and `reply` are target-based, not channel-based, so they remain separate commands.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
