#!/usr/bin/env bash
#
# Runs ON the EC2 host. Swaps in a freshly-uploaded binary and rolls the server
# to it. If the server is up, it does a **copyover** (SIGUSR2): the running
# process execs the new binary and hands over its live sockets, so connected
# players stay connected across the upgrade. If the server is down, it cold-starts.
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
    # Signal the process directly rather than via systemctl: the service runs as
    # this same user, so `kill` needs no privilege, and copyover is a process-level
    # operation (systemd keeps tracking it via the MAINPID handoff), not a restart.
    pid="$(systemctl show -p MainPID --value grim 2>/dev/null || true)"
    if [[ -n "$pid" && "$pid" != 0 ]]; then
        log "server up — copyover via SIGUSR2 -> $pid (players stay connected)"
        kill -USR2 "$pid"
        # The successor claims MAINPID and sends READY; the service stays active
        # throughout. Give it a moment, then confirm health.
        sleep 3
        if systemctl is-active --quiet grim; then
            log "copyover complete; server still active"
        else
            log "service inactive after copyover — falling back to cold start"
            sudo systemctl start grim
        fi
    else
        log "could not read MainPID — cold restart"
        sudo systemctl restart grim
    fi
else
    log "server down — cold start"
    sudo systemctl start grim
fi

log "done"
