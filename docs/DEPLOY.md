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
2. **deploy** — `scp`s the binary to `/opt/grim/bin/grim.new` plus the deploy
   scripts, then runs `deploy/deploy.sh` over SSH.

`deploy/deploy.sh` (on the host):

- If the server is **up**: log in over `localhost:4000` as an admin character
  and run `shutdown 30`, giving players a 30-second in-game countdown. The
  server saves-on-disconnect as usual, then exits 0.
- If the server is **already down**: skip the countdown.
- Swap `grim.new` into `grim`, then `systemctl start grim`.

The systemd unit uses **`Restart=on-failure`**: a clean admin shutdown (exit 0)
stays down long enough to swap the binary; only a crash auto-restarts. That is
what makes the graceful path work without a restart race.

## One-time EC2 setup

```bash
# As root on the box:
useradd --system --home /opt/grim --shell /usr/sbin/nologin grim
mkdir -p /opt/grim/bin /opt/grim/data
chown -R grim:grim /opt/grim
apt-get update && apt-get install -y expect telnet   # deploy.sh needs both

# Install the service unit (from this repo):
cp deploy/grim.service /etc/systemd/system/grim.service
systemctl daemon-reload
systemctl enable grim
```

Give the deploy SSH user passwordless sudo for just the two service actions
(`/etc/sudoers.d/grim-deploy`):

```
<deploy-user> ALL=(root) NOPASSWD: /bin/systemctl start grim, /bin/systemctl stop grim
```

First-ever start (no binary yet) will fail until the first deploy lands the
binary — that's expected.

## GitHub secrets

| Secret | What |
|---|---|
| `EC2_HOST` | Public hostname / IP of the box |
| `EC2_USER` | SSH user the deploy connects as (has the sudo rule above) |
| `EC2_SSH_KEY` | Private key (PEM) for that user |
| `ADMIN_LOGIN` | An **admin character name** to log in with for the graceful shutdown |
| `ADMIN_PASSWORD` | That account's password |

The admin credentials never leave the box — the countdown is triggered over
`localhost` telnet from inside `deploy.sh`.

## Granting admin

Roles live on the character JSON (`/opt/grim/data/characters/<name>.json`). To
make a character admin, add the role and restart:

```json
{
  "id": "…",
  "name": "Deployer",
  "roles": ["admin"]
}
```

`shutdown` is refused for any character without `"admin"` in `roles`.

> ⚠️ **Roles are granted by hand-editing JSON.** Wiping `data/` (which this
> project currently tolerates) drops the role, and the next deploy's graceful
> shutdown will fail to authenticate — falling back to `systemctl stop` (no
> player warning). Re-add the role after any wipe.

## Caveats

- **Port `4000` is hardcoded** in `crates/example-mud/src/main.rs`; `deploy.sh`
  assumes it.
- **No player-state flush on shutdown** beyond the normal save-on-disconnect —
  in-flight position changes can be lost across a restart. Acceptable for now.
- The telnet login in `trigger-shutdown.expect` is prompt-matched and therefore
  brittle; if the login flow changes, the graceful countdown degrades to a hard
  `systemctl stop` and the deploy still completes.
