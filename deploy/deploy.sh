#!/usr/bin/env bash
#
# Runs ON the EC2 host. Swaps in a freshly-uploaded binary and restarts GRIM.
# If the server is up, first signals it to shut down gracefully (SIGUSR1 →
# in-server countdown broadcast) so players get warned; if it is already down,
# skips straight to the swap.
#
# Expects the new binary staged at $STAGED (uploaded by the CI job).
set -euo pipefail

APP_DIR=/opt/grim
BIN="$APP_DIR/bin/grim"
STAGED="$APP_DIR/bin/grim.new"
# Must match SIGNAL_COUNTDOWN_SECS in crates/grim/src/plugins/shutdown.rs.
SHUTDOWN_SECS=30
GRACE_SECS=15

log() { echo "[deploy] $*"; }

[[ -f "$STAGED" ]] || { log "no staged binary at $STAGED"; exit 1; }

if systemctl is-active --quiet grim; then
    log "server up — signalling graceful shutdown (${SHUTDOWN_SECS}s countdown)"
    # Signal the process directly rather than `systemctl kill`: the service runs
    # as this same user, so `kill` needs no privilege — and it works even under a
    # least-privilege sudoers policy that only allows `systemctl start`/`stop`.
    pid="$(systemctl show -p MainPID --value grim 2>/dev/null || true)"
    if [[ -n "$pid" && "$pid" != 0 ]]; then
        kill -USR1 "$pid" || log "SIGUSR1 failed — will fall back to systemctl stop"
    else
        log "could not read MainPID — will fall back to systemctl stop"
    fi

    # The countdown exits the process cleanly (code 0); Restart=on-failure
    # leaves it down. Wait it out.
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
