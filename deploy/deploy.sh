#!/usr/bin/env bash
#
# Runs ON the EC2 host. Keeps the systemd unit in sync, swaps in the new binary,
# and rolls the server to it:
#
#   - unit changed → install it + one cold restart (the running instance must be
#                    under the new unit before a copyover can work).
#   - server UP    → **copyover** (SIGUSR2): the running process execs the new
#                    binary and hands over its live sockets; players stay
#                    connected. Nothing is stopped or restarted.
#   - server DOWN  → cold start.
#
# deploy.sh owning the unit is deliberate: a stale `Type=simple` unit makes a
# copyover look like the main process dying, so systemd stops the service (the
# in-game "restarting in 30s" countdown). Syncing the unit here stops that drift.
set -euo pipefail

APP_DIR=/opt/grim
BIN="$APP_DIR/bin/grim"
STAGED="$APP_DIR/bin/grim.new"
UNIT_SRC="$APP_DIR/bin/grim.service"          # uploaded by CI alongside the binary
UNIT_DST=/etc/systemd/system/grim.service

log() { echo "[deploy] $*"; }

[[ -f "$STAGED" ]] || { log "no staged binary at $STAGED"; exit 1; }

# Swap the new binary into place FIRST: on copyover the running process re-execs
# its own path (`current_exe`), so that path must already point at the new bytes.
log "swapping binary into place"
mv -f "$STAGED" "$BIN"
chmod +x "$BIN"

# Sync the systemd unit. Render `User=` to the deploy user so signalling the
# process needs no privilege. Only touch systemd if the unit actually changed.
unit_changed=0
if [[ -f "$UNIT_SRC" ]]; then
    rendered="$(mktemp)"
    sed "s#^User=.*#User=$(id -un)#" "$UNIT_SRC" > "$rendered"
    if ! sudo cmp -s "$rendered" "$UNIT_DST" 2>/dev/null; then
        log "installing/updating systemd unit at $UNIT_DST"
        sudo cp "$rendered" "$UNIT_DST"
        sudo systemctl daemon-reload
        sudo systemctl enable grim >/dev/null 2>&1 || true
        unit_changed=1
    fi
    rm -f "$rendered"
else
    log "warning: no grim.service uploaded — leaving the existing unit in place"
fi

if ! systemctl is-active --quiet grim; then
    log "server down — cold start"
    sudo systemctl start grim
elif [[ "$unit_changed" -eq 1 ]]; then
    # The live process is still under the OLD unit's semantics; a copyover can't
    # take effect until it restarts under the new unit. One-time on a unit change.
    log "unit changed — cold restart so the running instance adopts the new unit"
    sudo systemctl restart grim
else
    pid="$(systemctl show -p MainPID --value grim 2>/dev/null || true)"
    if [[ -n "$pid" && "$pid" != 0 ]]; then
        log "server up — copyover via SIGUSR2 -> $pid (players stay connected)"
        kill -USR2 "$pid"
    else
        log "server active but MainPID unavailable — ensuring it is started"
        sudo systemctl start grim
    fi
fi

log "done"
