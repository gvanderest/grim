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
| `trigger_copyover_on_due` | Update (chained, before poll) | `src/copyover.rs` | On an in-game `copyover` countdown's expiry (`CopyoverDue`), raise the same latched flag `SIGUSR2` raises. |
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
| `TelnetLimits` | Resource (line/buffer/rate/connect/shed caps, seconds for time; author-overridable) | `src/limits.rs` (`TelnetPlugin::with_limits`) |
| `NetworkBridge` | Resource (crate-internal) | `src/bridge.rs` |
| `CopyoverSignal` | Resource (crate-internal) | `src/copyover.rs` |
| `CopyoverDone` | Resource (crate-internal) | `src/copyover.rs` |

Wire messages (`ConnectionEstablished`, `ConnectionInput`, `ConnectionClosed`, `ConnectionOutput`, `ConnectionResumed`, `DisconnectRequest`) are defined in `grim-networking`; `TelnetPlugin` re-registers them so the transport can be used standalone.

## Notes

- `TelnetPlugin::new(port)` inserts `TelnetPort` plus default `TelnetLimits`, inits `CopyoverSignal` / `CopyoverDone`, and schedules the Startup + Update systems. `with_limits` overrides the caps. The five Update systems are `.chain()`-ed in order: drain → send → bridge copyover-due → poll copyover → finish copyover.
- Input guards (`src/guard.rs`, enforced in the read task before any byte reaches Bevy): lines past `max_line_len` truncate-and-deliver with the remainder discarded to the newline; a line past `max_buffer` without a newline, or more than `max_lines` per `rate_window_secs`, drops the connection with a `GuardTrip` reason logged alongside the peer address.
- Accept shed (`src/shed.rs`): past `max_connects_total` accepts per `total_window_secs` the listener sheds (accept-and-close, no handshake) for `shed_secs`; transitions cross to Bevy as `Shed` events (entry, ≤1/min heartbeats, lazy exit). Copyover re-adoptions bypass the gate.
- Drain alerts (`src/drain.rs`): connects/closes/resumes report `Logins` with the peer address; guard trips and shed transitions report `Security`. The read task stays Bevy-free — trips surface as `Disconnected { reason }`.
- Bevy's schedule is synchronous and tokio owns its threads, so the two are joined by a channel seam (`NetworkBridge`), not by running async TCP on Bevy's executor. See `docs/ARCHITECTURE.md` §5.1.
- IAC (`src/iac.rs`): minimal handshake (`IAC WILL ECHO`, `IAC WILL SUPPRESS_GO_AHEAD`) on fresh accept; `WILL_ECHO` / `WONT_ECHO` toggle password masking; `strip_iac` removes inbound `0xFF cmd cmd` sequences. Re-adopted copyover sockets skip the handshake.
- Rendering (`src/render.rs`): prepend a newline for unsolicited events, append the in-game `> ` prompt (`<AFK> ` while the session's `Client.afk` is set), convert colour codes to ANSI (via `grim-color`), translate `\n` → `\r\n`.
- Copyover / hot restart (`src/copyover.rs`, `src/server.rs`): `SIGUSR2` — or an admin's in-game `copyover [seconds]` countdown expiring into `CopyoverDue` — hands the live listener + in-game client sockets to a freshly-spawned successor over a unix socket (`SCM_RIGHTS`, via `sendfd`), waits for the ack, then exits. The `GRIM_COPYOVER_SOCK` env var tells a successor to adopt fds instead of binding fresh. See `docs/DEPLOY.md`.

---
*Format: [`docs/README.template.md`](../../docs/README.template.md). Improve over time.*
