#!/bin/sh
# Initialize a fresh Indexer database near the follower tip.
#
# This helper installs the pinned upstream schema and initializes Conduit's
# import cursor near the follower tip. The resulting database intentionally
# omits historical account/state data and must be used only for recent
# transaction history. Current ledger state continues to come from the
# participation algod node.

set -eu

# shellcheck source=docker/secret-utils.sh
. /secret-utils.sh

if [ "${INDEXER_LIGHT_BOOTSTRAP:-1}" != "1" ]; then
    echo "[indexer-bootstrap] lightweight bootstrap disabled"
    exit 0
fi

case "${INDEXER_BOOTSTRAP_SAFETY_ROUNDS:-64}" in
    ''|*[!0-9]*)
        echo "[indexer-bootstrap] ERROR: INDEXER_BOOTSTRAP_SAFETY_ROUNDS must be numeric"
        exit 1
        ;;
esac

export PGHOST="${PGHOST:-postgres}"
export PGPORT="${PGPORT:-5432}"
export PGUSER="${INDEXER_DB_USER:-algorand}"
export PGDATABASE="${INDEXER_DB_NAME:-indexer}"

FOLLOWER_URL="${ALGOD_FOLLOWER_URL:-http://algod-follower:8080}"
ALGOD_TOKEN_FILE="${ALGOD_TOKEN_FILE:-/run/secrets/algod_token}"
INDEXER_DB_PASSWORD_FILE="${INDEXER_DB_PASSWORD_FILE:-/run/secrets/indexer_db_password}"
SCHEMA_PATH="${INDEXER_SCHEMA_PATH:-/indexer-schema.sql}"

ALGOD_TOKEN=$(read_secret_file "$ALGOD_TOKEN_FILE" "algod API token")
INDEXER_DB_PASSWORD=$(read_secret_file \
    "$INDEXER_DB_PASSWORD_FILE" \
    "Indexer database password")
require_alphanumeric_secret "$ALGOD_TOKEN" "algod API token" 64 64
require_alphanumeric_secret \
    "$INDEXER_DB_PASSWORD" \
    "Indexer database password" \
    24 \
    128

PGPASSFILE=$(mktemp)
write_pgpass_file \
    "$PGPASSFILE" \
    "$PGHOST" \
    "$PGPORT" \
    "$PGDATABASE" \
    "$PGUSER" \
    "$INDEXER_DB_PASSWORD"
export PGPASSFILE
unset INDEXER_DB_PASSWORD
trap 'rm -f "$PGPASSFILE"' EXIT HUP INT TERM

if [ ! -r "$SCHEMA_PATH" ]; then
    echo "[indexer-bootstrap] ERROR: missing Indexer schema: $SCHEMA_PATH"
    exit 1
fi

echo "[indexer-bootstrap] waiting for follower and PostgreSQL"

for attempt in $(seq 1 180); do
    status=$(wget -qO- \
        --header="X-Algo-API-Token: ${ALGOD_TOKEN}" \
        "${FOLLOWER_URL}/v2/status" 2>/dev/null || true)
    versions=$(wget -qO- \
        --header="X-Algo-API-Token: ${ALGOD_TOKEN}" \
        "${FOLLOWER_URL}/versions" 2>/dev/null || true)
    follower_round=$(printf '%s' "$status" |
        sed -n 's/.*"last-round":\([0-9][0-9]*\).*/\1/p')
    catchpoint=$(printf '%s' "$status" |
        sed -n 's/.*"catchpoint":"\([^"]*\)".*/\1/p')
    genesis_hash=$(printf '%s' "$versions" |
        sed -n 's/.*"genesis_hash_b64":"\([^"]*\)".*/\1/p')

    if [ -z "$follower_round" ] || [ -n "$catchpoint" ] || [ -z "$genesis_hash" ]; then
        sleep 2
        continue
    fi

    schema_ready=$(psql -X -qAt -v ON_ERROR_STOP=1 \
        -c "SELECT to_regclass('public.metastate') IS NOT NULL" 2>/dev/null || true)
    if [ "$schema_ready" = "f" ]; then
        safety="${INDEXER_BOOTSTRAP_SAFETY_ROUNDS:-64}"
        if [ "$follower_round" -gt "$safety" ]; then
            start_round=$((follower_round - safety))
        else
            start_round=1
        fi

        if psql -X -q -v ON_ERROR_STOP=1 \
            --set=start_round="$start_round" \
            --set=genesis_hash="$genesis_hash" <<SQL
BEGIN;
\i ${SCHEMA_PATH}
INSERT INTO metastate (k, v) VALUES
    ('migration', jsonb_build_object('next', 20)),
    ('network', jsonb_build_object('genesis-hash', :'genesis_hash')),
    ('state', jsonb_build_object('next_account_round', :start_round))
ON CONFLICT (k) DO NOTHING;
COMMIT;
SQL
        then
            echo "[indexer-bootstrap] recent-history schema starts at round ${start_round}"
            exit 0
        fi
    elif [ "$schema_ready" = "t" ]; then
        state=$(psql -X -qAt -v ON_ERROR_STOP=1 -F '|' -c "
SELECT
    COALESCE((SELECT (v->>'next_account_round')::bigint FROM metastate WHERE k = 'state'), -1),
    COALESCE((SELECT count(*) FROM block_header), 0),
    COALESCE((SELECT (v->>'next')::integer FROM metastate WHERE k = 'migration'), -1)
" 2>/dev/null || true)

        next_round=$(printf '%s' "$state" | cut -d '|' -f 1)
        block_count=$(printf '%s' "$state" | cut -d '|' -f 2)
        migration=$(printf '%s' "$state" | cut -d '|' -f 3)

        if [ "$block_count" -gt 0 ] || [ "$next_round" -gt 0 ]; then
            echo "[indexer-bootstrap] existing Indexer state detected; no bootstrap change needed"
            exit 0
        fi

        if [ "$next_round" -eq 0 ]; then
            if [ "$migration" -ne 20 ]; then
                echo "[indexer-bootstrap] ERROR: unsupported Indexer schema migration $migration"
                exit 1
            fi

            safety="${INDEXER_BOOTSTRAP_SAFETY_ROUNDS:-64}"
            if [ "$follower_round" -gt "$safety" ]; then
                start_round=$((follower_round - safety))
            else
                start_round=1
            fi

            updated=$(psql -X -qAt -v ON_ERROR_STOP=1 \
                --set=start_round="$start_round" \
                --set=genesis_hash="$genesis_hash" -c "
UPDATE metastate
SET v = jsonb_build_object('next_account_round', :start_round)
WHERE k = 'state'
  AND (v->>'next_account_round')::bigint = 0
  AND NOT EXISTS (SELECT 1 FROM block_header)
RETURNING 1
" 2>/dev/null || true)

            if [ "$updated" = "1" ]; then
                psql -X -q -v ON_ERROR_STOP=1 \
                    --set=genesis_hash="$genesis_hash" -c "
INSERT INTO metastate (k, v)
VALUES ('network', jsonb_build_object('genesis-hash', :'genesis_hash'))
ON CONFLICT (k) DO UPDATE SET v = EXCLUDED.v
" >/dev/null
                echo "[indexer-bootstrap] recent-history import starts at round ${start_round}"
                exit 0
            fi
        fi
    fi

    if [ $((attempt % 15)) -eq 0 ]; then
        echo "[indexer-bootstrap] still waiting... (attempt ${attempt})"
    fi
    sleep 2
done

echo "[indexer-bootstrap] ERROR: timed out waiting for fresh Indexer initialization"
exit 1
