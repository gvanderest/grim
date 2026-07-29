#!/usr/bin/env bash
#
# Runs ON the EC2 host. Swaps in a freshly-uploaded binary and restarts GRIM.
# If the server is up, first asks it to shut down gracefully (in-game countdown)
# so players get warned; if it is already down, skips straight to the swap.
#
# Expects the new binary staged at $STAGED (uploaded by the CI job) and, for the
# graceful path, ADMIN_LOGIN / ADMIN_PASSWORD in the environment.
set -euo pipefail

APP_DIR=/opt/grim
BIN="$APP_DIR/bin/grim"
STAGED="$APP_DIR/bin/grim.new"
EXPECT="$APP_DIR/bin/trigger-shutdown.expect"
PORT=4000
SHUTDOWN_SECS=30
GRACE_SECS=15

log() { echo "[deploy] $*"; }

[[ -f "$STAGED" ]] || { log "no staged binary at $STAGED"; exit 1; }

if systemctl is-active --quiet grim; then
    log "server up — requesting graceful shutdown (${SHUTDOWN_SECS}s countdown)"
    if [[ -x "$EXPECT" && -n "${ADMIN_LOGIN:-}" && -n "${ADMIN_PASSWORD:-}" ]]; then
        "$EXPECT" "$PORT" "$ADMIN_LOGIN" "$ADMIN_PASSWORD" "$SHUTDOWN_SECS" \
            || log "in-game shutdown trigger failed — will fall back to systemctl stop"
    else
        log "expect script or admin creds unavailable — falling back to systemctl stop"
    fi

    # A clean admin shutdown exits 0; Restart=on-failure leaves it down. Wait it out.
    for _ in $(seq 1 $((SHUTDOWN_SECS + GRACE_SECS))); do
        systemctl is-active --quiet grim || break
        sleep 1
    done

    if systemctl is-active --quiet grim; then
        log "still running after grace period — forcing stop"
        sudo systemctl stop grim
    fi
else
    log "server already down — skipping countdown"
fi

log "swapping binary into place"
mv -f "$STAGED" "$BIN"
chmod +x "$BIN"

log "starting server"
sudo systemctl start grim
log "done"
