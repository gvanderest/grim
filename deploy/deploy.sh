#!/usr/bin/env bash
#
# Runs ON the EC2 host. Swaps in a freshly-uploaded binary and rolls the server
# to it:
#
#   - server UP   → **copyover** (SIGUSR2): the running process execs the new
#                   binary and hands over its live sockets, so connected players
#                   stay connected. Nothing is shut down or restarted.
#   - server DOWN → cold start.
#
# Expects the new binary staged at $STAGED (uploaded by the CI job).
set -euo pipefail

APP_DIR=/opt/grim
BIN="$APP_DIR/bin/grim"
STAGED="$APP_DIR/bin/grim.new"

log() { echo "[deploy] $*"; }

[[ -f "$STAGED" ]] || { log "no staged binary at $STAGED"; exit 1; }

# Swap the new binary into place FIRST: on copyover the running process re-execs
# its own path (`current_exe`), so that path must already point at the new bytes.
log "swapping binary into place"
mv -f "$STAGED" "$BIN"
chmod +x "$BIN"

if systemctl is-active --quiet grim; then
    # Copyover only. Signal the process directly (the service runs as this user,
    # so no privilege needed). The successor claims MAINPID + sends READY, so
    # systemd keeps tracking the service across the handoff — this is NOT a
    # restart, and the service is never stopped. Do not second-guess it with a
    # health re-check that could race the handoff and spuriously start a second
    # instance; if the copyover fails, the old process just keeps serving.
    pid="$(systemctl show -p MainPID --value grim 2>/dev/null || true)"
    if [[ -n "$pid" && "$pid" != 0 ]]; then
        log "server up — copyover via SIGUSR2 -> $pid (players stay connected)"
        kill -USR2 "$pid"
    else
        # Odd: reported active but no MainPID (e.g. the process exited between the
        # is-active and show queries). Don't silently report success — ensure the
        # new binary is actually running. `start` is a no-op if it's genuinely up.
        log "server active but MainPID unavailable — ensuring it is started"
        sudo systemctl start grim
    fi
else
    log "server down — cold start"
    sudo systemctl start grim
fi

log "done"
