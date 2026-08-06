# `grim-networking-telnet`
> Telnet transport for GRIM: TCP accept loop on a tokio thread, bridged to Bevy, with IAC negotiation, ANSI rendering, and copyover fd handoff.

**Role:** horizontal (infrastructure) — transport
**Depends on:** `grim-networking`, `grim-color`, `grim-core`, `grim-actor`, `bevy`, `tokio`, `serde_json`, `signal-hook`, `sendfd`, `sd-notify`, `rustix`

## Components
None. Reuses `grim_networking::Connection`.

## Systems
| System | Schedule | File | Purpose |
|---|---|---|---|
| `install_copyover_signal` | Startup | `src/copyover.rs` | Install the `SIGUSR2` handler that flips the copyover flag. |
| `start_telnet_server` | Startup | `src/server.rs` | Spawn the detached tokio thread: adopt inherited fds from a copyover predecessor or bind fresh, signal systemd readiness, run the accept/command `select!` loop. |
| `drain_network_events` | Update (chained) | `src/bridge.rs` | Drain events off the tokio→Bevy channel into `ConnectionEstablished` / `ConnectionResumed` / `ConnectionInput` / `ConnectionClosed` messages, spawning/despawning `Connection` entities. |
| `send_network_commands` | Update (chained) | `src/bridge.rs` | Read `ConnectionOutput` / `DisconnectRequest` and route them back to the network thread (render + echo toggle + disconnect). |
| `poll_copyover_signal` | Update (chained) | `src/copyover.rs` | On a raised `SIGUSR2` flag, snapshot in-game sessions into a `HandoverManifest` and start the handoff to the successor. |
| `finish_copyover` | Update (chained) | `src/copyover.rs` | Once the successor acks the handoff, emit `AppExit` to exit the predecessor cleanly. |

## Commands
None. This is a transport; it produces `ConnectionInput` messages, not game commands.

| Command | Handler | Summary |
|---|---|---|
| None | — | — |

## Resources & Events
| Name | Kind (Resource/Message) | File |
|---|---|---|
| `TelnetPort` | Resource | `src/bridge.rs` |
| `NetworkBridge` | Resource (crate-internal) | `src/bridge.rs` |
| `CopyoverSignal` | Resource (crate-internal) | `src/copyover.rs` |
| `CopyoverDone` | Resource (crate-internal) | `src/copyover.rs` |

Wire messages (`ConnectionEstablished`, `ConnectionInput`, `ConnectionClosed`, `ConnectionOutput`, `ConnectionResumed`, `DisconnectRequest`) are defined in `grim-networking`; `TelnetPlugin` re-registers them so the transport can be used standalone.

## Notes
- `TelnetPlugin::new(port)` inserts `TelnetPort`, inits `CopyoverSignal` / `CopyoverDone`, and schedules the Startup + Update systems. The four Update systems are `.chain()`-ed in order: drain → send → poll copyover → finish copyover.
- Bevy's schedule is synchronous and tokio owns its threads, so the two are joined by a channel seam (`NetworkBridge`), not by running async TCP on Bevy's executor. See `docs/ARCHITECTURE.md` §5.1.
- IAC (`src/iac.rs`): minimal handshake (`IAC WILL ECHO`, `IAC WILL SUPPRESS_GO_AHEAD`) on fresh accept; `WILL_ECHO` / `WONT_ECHO` toggle password masking; `strip_iac` removes inbound `0xFF cmd cmd` sequences. Re-adopted copyover sockets skip the handshake.
- Rendering (`src/render.rs`): prepend a newline for unsolicited events, append the in-game `> ` prompt, convert colour codes to ANSI (via `grim-color`), translate `\n` → `\r\n`.
- Copyover / hot restart (`src/copyover.rs`, `src/server.rs`): `SIGUSR2` hands the live listener + in-game client sockets to a freshly-spawned successor over a unix socket (`SCM_RIGHTS`, via `sendfd`), waits for the ack, then exits. The `GRIM_COPYOVER_SOCK` env var tells a successor to adopt fds instead of binding fresh. See `docs/DEPLOY.md`.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
