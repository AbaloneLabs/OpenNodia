#!/bin/sh
# Keep upstream Indexer transaction tables within a fixed recent-round window.

set -eu

# shellcheck source=docker/secret-utils.sh
. /secret-utils.sh

case "${INDEXER_HISTORY_ROUNDS:-20000}" in
    ''|*[!0-9]*)
        echo "[indexer-pruner] ERROR: INDEXER_HISTORY_ROUNDS must be numeric"
        exit 1
        ;;
esac
case "${INDEXER_PRUNE_INTERVAL_SECONDS:-300}" in
    ''|*[!0-9]*)
        echo "[indexer-pruner] ERROR: INDEXER_PRUNE_INTERVAL_SECONDS must be numeric"
        exit 1
        ;;
esac

if [ "${INDEXER_HISTORY_ROUNDS:-20000}" -lt 1000 ]; then
    echo "[indexer-pruner] ERROR: INDEXER_HISTORY_ROUNDS must be at least 1000"
    exit 1
fi
if [ "${INDEXER_PRUNE_INTERVAL_SECONDS:-300}" -lt 30 ]; then
    echo "[indexer-pruner] ERROR: INDEXER_PRUNE_INTERVAL_SECONDS must be at least 30"
    exit 1
fi

export PGHOST="${PGHOST:-postgres}"
export PGPORT="${PGPORT:-5432}"
export PGUSER="${INDEXER_DB_USER:-algorand}"
export PGDATABASE="${INDEXER_DB_NAME:-indexer}"

INDEXER_DB_PASSWORD_FILE="${INDEXER_DB_PASSWORD_FILE:-/run/secrets/indexer_db_password}"
INDEXER_DB_PASSWORD=$(read_secret_file \
    "$INDEXER_DB_PASSWORD_FILE" \
    "Indexer database password")
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

echo "[indexer-pruner] retaining ${INDEXER_HISTORY_ROUNDS} recent rounds"

while true; do
    if psql -X -q -v ON_ERROR_STOP=1 \
        --set=retention="${INDEXER_HISTORY_ROUNDS}" <<'SQL'
BEGIN;

CREATE TEMP TABLE opennodia_prune_cutoff (
    cutoff bigint NOT NULL
) ON COMMIT DROP;

INSERT INTO opennodia_prune_cutoff (cutoff)
SELECT GREATEST(0, COALESCE(MAX(round), 0) - :retention + 1)
FROM block_header;

DELETE FROM txn_participation
USING opennodia_prune_cutoff
WHERE txn_participation.round < opennodia_prune_cutoff.cutoff;

DELETE FROM txn
USING opennodia_prune_cutoff
WHERE txn.round < opennodia_prune_cutoff.cutoff;

DELETE FROM block_header
USING opennodia_prune_cutoff
WHERE block_header.round < opennodia_prune_cutoff.cutoff;

INSERT INTO metastate (k, v)
SELECT
    'pruned',
    jsonb_build_object(
        'last_pruned',
        to_char(
            clock_timestamp() AT TIME ZONE 'UTC',
            'YYYY-MM-DD"T"HH24:MI:SS"Z"'
        ),
        'oldest_txn_round',
        cutoff
    )
FROM opennodia_prune_cutoff
ON CONFLICT (k) DO UPDATE SET v = EXCLUDED.v;

COMMIT;

ALTER TABLE txn SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_analyze_scale_factor = 0.01
);
ALTER TABLE txn_participation SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_analyze_scale_factor = 0.01
);
ALTER TABLE block_header SET (
    autovacuum_vacuum_scale_factor = 0.02,
    autovacuum_analyze_scale_factor = 0.01
);
SQL
    then
        echo "[indexer-pruner] prune cycle completed"
    else
        echo "[indexer-pruner] schema unavailable or prune failed; retrying"
    fi

    sleep "${INDEXER_PRUNE_INTERVAL_SECONDS}"
done
