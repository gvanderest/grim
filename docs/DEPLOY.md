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

- If the server is **up**: `systemctl kill -s SIGUSR1 grim`. The server traps
  `SIGUSR1` and runs a 30-second countdown, broadcasting warnings to every
  connected player, then exits 0.
- If the server is **already down**: skip the countdown.
- Swap `grim.new` into `grim`, then `systemctl start grim`.

The systemd unit uses **`Restart=on-failure`**: a clean shutdown (exit 0) stays
down long enough to swap the binary; only a crash auto-restarts. That is what
makes the graceful path work without a restart race.

The graceful shutdown needs **no login and no credentials** — the deploy just
signals the process. (An admin can also trigger the same countdown in-game with
`shutdown <seconds>`; see below.)

## One-time EC2 setup

On Amazon Linux the deploy connects as `ec2-user`, which already has passwordless
sudo, so it also runs the service — no separate service user needed.

```bash
sudo mkdir -p /opt/grim/bin /opt/grim/data
sudo chown -R ec2-user:ec2-user /opt/grim

sudo tee /etc/systemd/system/grim.service >/dev/null <<'EOF'
[Unit]
Description=GRIM MUD server
After=network.target

[Service]
Type=simple
User=ec2-user
WorkingDirectory=/opt/grim
ExecStart=/opt/grim/bin/grim
Restart=on-failure
RestartSec=2

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable grim
```

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

The deploy does not need this — it uses `SIGUSR1`. But the in-game `shutdown`
command is gated on an `admin` role. Roles live on the character JSON
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
- **No player-state flush on shutdown** beyond the normal save-on-disconnect —
  in-flight position changes can be lost across a restart. Acceptable for now.
- `SHUTDOWN_SECS` in `deploy.sh` must match `SIGNAL_COUNTDOWN_SECS` in
  `crates/grim/src/plugins/shutdown.rs` (both `30`).
