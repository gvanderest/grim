# Deployment

Every push to `main` builds a static binary in GitHub Actions and rolls it onto
a single EC2 host. No Docker, and nothing is compiled on the box — a t2.micro
(1 GB RAM) cannot realistically build Bevy in release, so CI does it and ships
the binary.

## Pipeline

`.github/workflows/deploy.yml`:

1. **build** — `cargo build --release --target x86_64-unknown-linux-musl`. The
   musl target produces a fully static binary (assets are `include_str!`-baked,
   so it needs no files beside it). Uploaded as an artifact named `grim`.
2. **deploy** — `scp`s the binary to `/opt/grim/bin/grim.new` plus
   `deploy/deploy.sh`, then runs the script over SSH.

`deploy/deploy.sh` (on the host):

- **Swap `grim.new` into `grim` first.** On copyover the running process re-execs
  its own path, so that path must already hold the new binary.
- If the server is **up**: `kill -USR2 <MainPID>` — a **copyover** (hot restart).
  The process execs the new binary and hands its live listener + player sockets
  to the successor over a unix socket (SCM_RIGHTS); the successor reloads the
  world from disk and drops each player back into the room they were in, with **no
  disconnect and no re-login**. Signalling directly needs no privilege (the
  service runs as the deploy user).
- If the server is **down**: `systemctl start grim` (cold start).

### How copyover survives systemd

The unit is **`Type=notify`** with **`NotifyAccess=all`**. On startup the server
sends `READY=1`. During a copyover the successor also sends `MAINPID=<its pid>`
*before* the predecessor exits, so systemd follows the handoff instead of seeing
the old MainPID die and treating it as a crash. `Restart=on-failure` still means a
real crash (non-zero exit) restarts, while a clean admin/SIGTERM shutdown stays
down for a cold deploy.

Only **actively-playing** sessions (in-game, not linkdead) carry across; anyone at
the login prompt reconnects fresh. If a character can't be cleanly restored on the
far side, that one socket is dropped rather than logged into a bad state.

### Graceful shutdown (SIGTERM)

`systemctl stop` (and thus a manual stop) sends **`SIGTERM`**, which the server
traps to run a 30-second countdown, broadcasting warnings to every player, then
exits 0. `TimeoutStopSec=45` gives the countdown room before systemd would
`SIGKILL`. No login or credentials needed. (An admin can trigger the same
countdown in-game with `shutdown <seconds>`; see below.)

## One-time EC2 setup

On Amazon Linux the deploy connects as `ec2-user`, which already has passwordless
sudo, so it also runs the service — no separate service user needed.

**`deploy.sh` manages the systemd unit** (`deploy/grim.service`): it installs/updates
it on every deploy — rendering `User=` to the deploy user — and `daemon-reload`s +
cold-restarts once whenever the unit changes. So the box just needs the directory
and the user's sudo; the first deploy installs and starts the service:

```bash
sudo mkdir -p /opt/grim/bin /opt/grim/data
sudo chown -R ec2-user:ec2-user /opt/grim
```

The unit is **`Type=notify` + `NotifyAccess=all`** — this is what makes copyover
work. Under a stale `Type=simple` unit, a copyover looks like the main process
dying, so systemd stops the service (you'd see the in-game "restarting in 30s"
countdown); `deploy.sh` syncing the unit prevents that drift.

`deploy/grim.service` in the repo is the same unit with `User=grim`; edit the
`User=` line if you run it under a dedicated user with a non-default deploy
account instead.

First-ever start fails until the first deploy lands the binary — expected. Don't
`systemctl start` by hand; let the deploy do it.

Also open **TCP 4000** inbound in the instance's security group.

## GitHub secrets

| Secret | What |
|---|---|
| `EC2_HOST` | Public hostname / IP of the box |
| `EC2_USER` | SSH user the deploy connects as (`ec2-user` on Amazon Linux) |
| `EC2_SSH_KEY` | Private key (PEM) for that user |

The binary itself reads no configuration — port `4000` is compiled in.

## Admin role (for the in-game `shutdown` command)

The deploy does not need this — it uses `SIGUSR2` (copyover) / `SIGTERM`
(shutdown). But the in-game `shutdown` command is gated on an `admin` role. Roles
live on the character JSON
(`/opt/grim/data/characters/<name>.json`):

```json
{
  "id": "…",
  "name": "Deployer",
  "roles": ["admin"]
}
```

Granting is a manual JSON edit; a `data/` wipe drops it. A non-admin running
`shutdown` gets the ordinary unknown-command response — the command's existence
is not disclosed.

## Caveats

- **Port `4000` is hardcoded** in `crates/example-mud/src/main.rs`.
- **Copyover reloads the world from scratch** — only persisted accounts/characters
  carry over, not transient world state. A character's room *is* persisted (written
  to disk on every move), so players resume where they were.
- **Copyover is Unix-only** (POSIX signals + fd passing). On other platforms a
  restart is a plain cold restart.
- **A crash (SIGKILL, panic) still loses** anything since the last disk write.
  Durable persistence (WAL + autosave) is planned; today the guarantees are
  save-on-disconnect, save-on-quit, and save-on-move.
