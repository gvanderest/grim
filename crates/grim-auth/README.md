# `grim-auth`
> The pre-game phase: login, account/character creation, character selection, and the MOTD — everything a session does before entering the world.

**Role:** vertical — pre-game authentication & character onboarding
**Depends on:** `grim-core`, `grim-scene`, `grim-actor`, `grim-world`, `grim-persistence`, `grim-networking`, `grim-text`, `grim-color`

Layered on the session core: `grim-auth → grim-scene` (it reuses the scene's
shared render helpers and `ConnectedAt`/`JustEnteredWorld`/`SceneSystems`). The
reverse edge is forbidden — `grim-scene` has zero references to `grim-auth`.
Still driven by `grim_core::ClientState`; the typed scene-stack model remains
deferred (ARCHITECTURE.md §5.3/§8).

## Components
`grim-auth` defines no components of its own. It reads/writes the beings
(`Account` on `grim-core`; `Character`/`Player`/`InRoom`/`Linkdead`/`OutputHistory`
on `grim-actor`) and stamps `grim_scene::ConnectedAt` when a session enters the
world.

## Systems

| System | Schedule | File | Purpose |
|---|---|---|---|
| `handle_connection_established` | `Update` | `src/greeter.rs` | Spawns a `Client`, prints the login banner + first prompt — the entry to the flow. |
| `validate_registries` | `Startup` | `src/plugin.rs` | Panics on a mis-seeded world (empty race registry / no tier-1 class) so creation can't trap a player. |
| `handle_pregame_input` | `Update` (`.after` the greeter, `.before(SceneSystems::InGameInput)`) | `src/input.rs` | Routes a line by pre-game `ClientState`; skips `InGame` (scene's job); records connections it advances into the world in `JustEnteredWorld`. |

## State handlers
The pre-game flow is a state machine, one handler file per concern (not the
one-file-per-command layout, which is for in-game command handlers).

| `ClientState` | Handler | File |
|---|---|---|
| `LoginPrompt` | `login_prompt` | `src/login.rs` |
| `ConfirmCreate` | `confirm_create` | `src/login.rs` |
| `PasswordPrompt` | `password_prompt` (→ `create_account` / `authenticate`) | `src/login.rs` |
| `CharacterSelect` | `character_select` (+ `show_character_menu`, `account_character_list`) | `src/character_select.rs` |
| `CreateCharacter` | `create_character` | `src/creation.rs` |
| `SelectGender` / `SelectRace` / `SelectClass` | `select_gender` / `select_race` / `select_class` | `src/creation.rs` |
| `MotdPrompt` | `motd_prompt` (advances to `InGame`, auto-looks the room) | `src/character_select.rs` |

Entering the world (reconnect linkdead / take over resident / spawn from disk)
is `enter_world_by_name` in `src/world_entry.rs`, shared by the login-by-name,
character-select, and legacy-backfill (class-pick) paths. Completing a new build
is `finalize_character` / `backfill_and_enter` in `src/finalize.rs`.

## Resources & Events

| Name | Kind (Resource/Message) | File |
|---|---|---|
| `ReservedNamePrefixes` | Resource (character-name prefix blocklist; author-overridable) | `src/validation.rs` |
| `RaceRegistry` / `ClassRegistry` | Resource (read for the creation menus; `init_resource`, from `grim-world`) | `src/plugin.rs` |
| `PersistenceConfig` | Resource (account/character JSON dir; `init_resource`, from `grim-persistence`) | `src/plugin.rs` |
| `LoginAnnounce` / `LinkdeadAnnounce` | Message (emitted on world entry) | `src/character_select.rs`, `src/world_entry.rs` |
| `LookRoom` | Message (emitted at MOTD to auto-look) | `src/character_select.rs` |
| `ConnectionOutput` / `DisconnectRequest` | Message (from `grim-networking`) | throughout |

Input validation lives entirely in `src/validation.rs`: `hash_password` /
`verify_password`, `validate_identifier` (email), `validate_character_name` /
`normalize_character_name`, `validate_password`, `is_name_reserved`, plus the
`ReservedNamePrefixes` resource and `ValidationError`.

## Notes
- Single plugin: `AuthPlugin` — one plugin registering the greeter, the pre-game
  input system, the reserved-name / race / class / persistence resources, and the
  mis-seed guard.
- **Routing coordination.** The pre-game system runs before the scene in-game
  system (`SceneSystems::InGameInput`). Because a single line can transition a
  session to `InGame`, the pre-game system records that connection in
  `grim_scene::JustEnteredWorld` (cleared each tick) so the scene system does not
  re-dispatch the triggering line — verified by the `example-mud` login E2E
  scenarios.
- `grim-auth` defines its own `SystemParam` bundles (`src/params.rs`) over the
  underlying components/resources rather than reusing the scene's, so the two
  crates stay decoupled; only genuinely shared render helpers
  (`grim_scene::formatter`) are called across the boundary.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
