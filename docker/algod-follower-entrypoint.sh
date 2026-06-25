#!/bin/sh
# Wrapper entrypoint for the algod FOLLOWER node used by Conduit.
#
# Differences from the participation entrypoint (algod-entrypoint.sh):
#   1. Activates the "conduit" profile before starting algod. This sets
#      EnableFollowMode=true (and related follower settings), so the node
#      does NOT participate in consensus, cannot propose blocks, and cannot
#      submit transactions. It only serves block data to Conduit.
#   2. Does NOT start kmd (the follower never submits transactions, so key
#      management is unnecessary).
#   3. Starts from its persisted ledger round and only retains the configured
#      account lookback needed to serve recent deltas to Conduit.
#
# The participation node (separate container) keeps proposing blocks and
# earning rewards as usual. This follower is a lightweight, read-only data
# source for the Conduit pipeline.

set -eu

# shellcheck source=docker/secret-utils.sh
. /secret-utils.sh

TOKEN_FILE="${TOKEN_FILE:-/run/secrets/algod_token}"
TOKEN=$(read_secret_file "$TOKEN_FILE" "algod API token")
require_alphanumeric_secret "$TOKEN" "algod API token" 64 64
export TOKEN

READY_FILE="/tmp/follower-ready"
rm -f "$READY_FILE"

# Wait for the algod data directory to exist (created by the base image
# on first boot).
while [ ! -d /algod/data ]; do
    sleep 1
done

# Give the base image a moment to finish initializing the data directory
# (genesis, config.json, etc.).
sleep 3

# Activate the Conduit follower profile. This command writes the follower
# settings into config.json:
#   EnableFollowMode = true
#   MaxAcctLookback  = FOLLOWER_MAX_ACCT_LOOKBACK (default: 2000)
#   CatchupParallelBlocks = 64
# It must run AFTER the data directory is initialized but BEFORE algod
# starts accepting connections.
if command -v algocfg >/dev/null 2>&1; then
    su algorand -c "algocfg profile set conduit -y -d /algod/data" || {
        echo "[follower] ERROR: algocfg profile set failed"
        exit 1
    }

    FOLLOWER_MAX_ACCT_LOOKBACK="${FOLLOWER_MAX_ACCT_LOOKBACK:-2000}"
    case "$FOLLOWER_MAX_ACCT_LOOKBACK" in
        ''|*[!0-9]*)
            echo "[follower] ERROR: FOLLOWER_MAX_ACCT_LOOKBACK must be numeric"
            exit 1
            ;;
    esac
    if [ "$FOLLOWER_MAX_ACCT_LOOKBACK" -lt 64 ]; then
        echo "[follower] ERROR: FOLLOWER_MAX_ACCT_LOOKBACK must be at least 64"
        exit 1
    fi
    su algorand -c "algocfg set -d /algod/data -p MaxAcctLookback -v ${FOLLOWER_MAX_ACCT_LOOKBACK}" || {
        echo "[follower] ERROR: failed to configure MaxAcctLookback"
        exit 1
    }
else
    echo "[follower] WARNING: algocfg not found, follower mode may not be active"
fi

# Mark the follower ready once algod is serving status requests and no
# catchpoint installation is active.
(
    # Wait for algod to be ready to accept commands.
    for i in $(seq 1 60); do
        if goal node status >/dev/null 2>&1; then
            break
        fi
        sleep 2
    done
    for i in $(seq 1 120); do
        STATUS=$(curl -fsS \
            -H "X-Algo-API-Token: ${TOKEN}" \
            http://127.0.0.1:8080/v2/status 2>/dev/null || true)
        ROUND=$(printf '%s' "$STATUS" | sed -n 's/.*"last-round":\([0-9][0-9]*\).*/\1/p')
        CATCHPOINT=$(printf '%s' "$STATUS" | sed -n 's/.*"catchpoint":"\([^"]*\)".*/\1/p')
        if [ -n "$ROUND" ] && [ -z "$CATCHPOINT" ]; then
            if [ "$ROUND" -eq 0 ]; then
                READY_URL="http://127.0.0.1:8080/v2/blocks/0?format=msgpack"
            else
                READY_URL="http://127.0.0.1:8080/v2/deltas/${ROUND}?format=msgp"
            fi
            if curl -fsS \
                -H "X-Algo-API-Token: ${TOKEN}" \
                "$READY_URL" \
                >/dev/null 2>&1; then
                touch "$READY_FILE"
                echo "[follower] ready at round ${ROUND}"
                exit 0
            fi
        fi
        if [ $((i % 12)) -eq 0 ]; then
            echo "[follower] waiting for algod readiness... (attempt ${i})"
        fi
        sleep 5
    done
    echo "[follower] ERROR: algod did not become ready before the timeout"
) &

# Replace this shell with the real algod entrypoint (becomes PID 1).
# The base image's run.sh starts algod, which will catch up via the
# follower configuration above. Conduit coordinates the required sync round.
exec /node/run/run.sh
