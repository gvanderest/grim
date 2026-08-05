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
AREAS_STAGED="$APP_DIR/bin/areas.staged"      # committed world content from CI
AREAS_DST="$APP_DIR/data/areas"               # WorkingDirectory=/opt/grim, seed reads data/areas

log() { echo "[deploy] $*"; }

# Both artifacts are required up front. Bail BEFORE swapping the binary if the
# unit is missing (a broken upload), so we never install a new binary under a
# stale/absent unit.
[[ -f "$STAGED" ]] || { log "no staged binary at $STAGED"; exit 1; }
[[ -f "$UNIT_SRC" ]] || { log "no unit at $UNIT_SRC (broken upload)"; exit 1; }

# Swap the new binary into place FIRST: on copyover the running process re-execs
# its own path (`current_exe`), so that path must already point at the new bytes.
log "swapping binary into place"
mv -f "$STAGED" "$BIN"
chmod +x "$BIN"

# Sync committed world content (area blueprints) into data/areas BEFORE the roll:
# the server reads them from disk at startup, and a copyover re-seeds, so they
# must be current before we signal. data/ holds runtime player saves; only the
# areas/ subdir is managed here. Missing staged dir → leave existing areas
# untouched (don't wipe a working world over a partial upload).
if [[ -d "$AREAS_STAGED" ]]; then
    # Full mirror: wipe the whole dir and repopulate, so an area removed or
    # renamed in the repo doesn't linger on the box. Only data/areas is cleared
    # — sibling data/accounts and data/characters (runtime saves) are untouched.
    log "replacing area blueprints in $AREAS_DST"
    # Stage into a temp dir and only swap it in on a clean copy. Deleting the
    # live dir first and ignoring a failed cp could leave the world with empty
    # or partial area data — the next startup/copyover would then have no world.
    AREAS_TMP="${AREAS_DST}.new.$$"
    rm -rf "$AREAS_TMP"
    mkdir -p "$AREAS_TMP"
    if ! cp "$AREAS_STAGED"/*.json "$AREAS_TMP"/; then
        log "ERROR: failed to copy staged area blueprints — leaving $AREAS_DST untouched"
        rm -rf "$AREAS_TMP" "$AREAS_STAGED"
        exit 1
    fi
    rm -rf "$AREAS_DST"
    mv "$AREAS_TMP" "$AREAS_DST"
    rm -rf "$AREAS_STAGED"
else
    log "no staged area blueprints — leaving $AREAS_DST as-is"
fi

# Sync the systemd unit. Render `User=` to the deploy user so signalling the
# process needs no privilege.
#
# Adoption is tracked with a PERSISTENT marker, not just an in-run flag: the
# running process only adopts a new unit when it restarts, so between "unit
# installed + daemon-reloaded" and "restarted" there is a window where the unit
# on disk is new but the live process is still under the old one. If the deploy
# is interrupted in that window, a later run would see the unit unchanged and
# wrongly copyover into a process running the old (e.g. Type=simple) lifecycle.
# The marker survives that interruption and forces the pending cold restart.
PENDING="$APP_DIR/.unit-restart-pending"
rendered="$(mktemp)"
sed "s#^User=.*#User=$(id -un)#" "$UNIT_SRC" > "$rendered"
if ! sudo cmp -s "$rendered" "$UNIT_DST" 2>/dev/null; then
    log "installing/updating systemd unit at $UNIT_DST"
    sudo cp "$rendered" "$UNIT_DST"
    sudo systemctl daemon-reload
    touch "$PENDING"   # a restart is now owed; persists until one succeeds
fi
rm -f "$rendered"
# Ensure the service is enabled for boot on EVERY deploy (idempotent), not just
# when the unit changed — otherwise an already-installed-but-disabled unit leaves
# GRIM down after the next host reboot. Not suppressed: a real enable failure
# should fail the deploy, not report success.
sudo systemctl enable grim >/dev/null

if ! systemctl is-active --quiet grim; then
    log "server down — cold start"
    sudo systemctl start grim
    rm -f "$PENDING"
elif [[ -f "$PENDING" ]]; then
    # A unit change hasn't been adopted yet (this run's, or an interrupted prior
    # run's). Cold restart so the running instance picks it up before any copyover.
    log "unit change pending adoption — cold restart"
    sudo systemctl restart grim
    rm -f "$PENDING"
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
