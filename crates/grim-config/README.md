# `grim-config`
> Player config registry: named settings with valid values and defaults.

**Role:** horizontal (infrastructure) — player settings
**Depends on:** nothing (Bevy `Resource` only)

Plugins register a `ConfigDef` (key, accepted values, fallback default,
scope); each character's chosen values live on its `Character.config` map
(`grim-actor`, persisted via `StoredCharacter`). Readers resolve through
`ConfigRegistry::resolve`, which applies the stored choice only when
registered and valid and otherwise falls back to the default — a hand-edited
or stale value can never break the reader.

## Components

None — settings are data (`ConfigDef`), choices live on `grim-actor`.

## Systems

None — no plugin (plain library + resource, ARCHITECTURE.md §2). The owning
feature seeds its settings at startup (e.g. `minimap` in `ActorPlugin`); the
`config` verb reads/writes through the registry.

## Commands

None here — `config` is parsed by `grim-scene` and handled in `grim-actor`
(`src/commands/config.rs`).

## Resources & Events

| Name | Kind (Resource/Message) | File |
|---|---|---|
| `ConfigRegistry` | Resource | `src/registry.rs` |
| `Scope` | Value (`Character`, account reserved) | `src/registry.rs` |

## Types

| Type | Kind | File | Purpose |
|---|---|---|---|
| `ConfigDef` | Value (`key` + `valid` + `default` + `scope`) | `src/registry.rs` | One registered setting; `canonical()` matches a raw value case-insensitively. |

## Notes

- Resolution never writes back: an invalid stored value resolves to the
  default on every read and stays on disk until the player sets it.
- Key lookup is exact-match; callers (the `config` verb) normalize case first.
- `Scope::Account` arrives with the first account-wide setting (ADR-0007 §4),
  never ahead of it.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
