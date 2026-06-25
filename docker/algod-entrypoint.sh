#!/bin/sh
# Wrapper entrypoint for algod that:
#   1. Syncs kmd token files to a shared volume (kmd-v0.5 dir is mode 700)
#   2. Ensures kmd is running
#   3. Replaces itself with the real algod entrypoint (becomes PID 1)

set -eu

# shellcheck source=docker/secret-utils.sh
. /secret-utils.sh

TOKEN_FILE="${TOKEN_FILE:-/run/secrets/algod_token}"
TOKEN=$(read_secret_file "$TOKEN_FILE" "algod API token")
require_alphanumeric_secret "$TOKEN" "algod API token" 64 64
export TOKEN

if command -v algocfg >/dev/null 2>&1; then
    su algorand -c "algocfg set -d /algod/data -p EnableDeveloperAPI -v true" || {
        echo "[algod] ERROR: failed to enable the local TEAL developer API" >&2
        exit 1
    }
else
    echo "[algod] ERROR: algocfg is required to enable TEAL validation" >&2
    exit 1
fi

# Background task: ensure kmd is running and sync tokens periodically
maintain_kmd() {
    # Wait for algod data dir to be ready
    while [ ! -d /algod/data ]; do
        sleep 1
    done

    # Give algod a moment to initialize
    sleep 3

    mkdir -p /kmd-shared
    chown "${OPENNODIA_UID:-1001}:${OPENNODIA_UID:-1001}" /kmd-shared
    chmod 700 /kmd-shared

    while true; do
        # Check if kmd is running; if not, start it
        if [ "$START_KMD" = "1" ]; then
            if ! kill -0 "$(cat /algod/data/kmd-v0.5/kmd.pid 2>/dev/null)" 2>/dev/null; then
                su algorand -c "goal kmd start -d /algod/data" 2>/dev/null
                sleep 2
            fi
        fi

        # Sync token files to shared volume
        if [ -f /algod/data/kmd-v0.5/kmd.token ]; then
            cp /algod/data/kmd-v0.5/kmd.token /kmd-shared/kmd.token 2>/dev/null
            cp /algod/data/kmd-v0.5/kmd.net /kmd-shared/kmd.net 2>/dev/null
            chown "${OPENNODIA_UID:-1001}:${OPENNODIA_UID:-1001}" \
                /kmd-shared/kmd.token /kmd-shared/kmd.net 2>/dev/null
            chmod 600 /kmd-shared/kmd.token /kmd-shared/kmd.net 2>/dev/null
        fi

        sleep 5
    done
}

# Start kmd maintenance in the background
maintain_kmd &

# Replace this shell with the real algod entrypoint (becomes PID 1)
exec /node/run/run.sh
