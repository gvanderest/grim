# `grim-color`
> Transport-independent colour markup and its rendering to ANSI.

**Role:** horizontal (infrastructure) — pure functions
**Depends on:** nothing (std only). No Bevy, no serde. Consumed by `grim-text` and by the terminal transports (telnet/SSH).

## Components
None. Pure library — no Bevy dependency, no ECS types.

## Systems
None. No `Plugin`, no `add_systems`.

## Commands
None.

## Resources & Events
None.

## Notes
- Public API (`src/lib.rs`): `ansi` (markup → ANSI escapes, `src/render.rs`), `convert_16color` (`{`-family → `@x`-family via GRIM's palette, `src/convert.rs`), `escape_codes` (doubles markup introducers so untrusted values render literally, `src/escape.rs`), and the `palette` module (`src/palette.rs`).
- Two markup families: 16-colour `{`+char codes, and 24-bit `@x`/`@b`+hex codes.
- Deliberately std-only: it is the one piece testable without an `App`, and terminal transports depend on it for rendering while WebSocket simply does not (ARCHITECTURE.md §2, §5.1). The palette is compile-time constants; making it runtime-configurable is deferred (ARCHITECTURE.md §9).

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
