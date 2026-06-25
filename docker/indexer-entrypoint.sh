#!/bin/sh
# Start the Indexer from an ephemeral configuration containing DB credentials.

set -eu

# shellcheck source=docker/secret-utils.sh
. /secret-utils.sh

INDEXER_DB_HOST="${INDEXER_DB_HOST:-postgres}"
INDEXER_DB_PORT="${INDEXER_DB_PORT:-5432}"
INDEXER_DB_USER="${INDEXER_DB_USER:-algorand}"
INDEXER_DB_NAME="${INDEXER_DB_NAME:-indexer}"
INDEXER_DB_PASSWORD_FILE="${INDEXER_DB_PASSWORD_FILE:-/run/secrets/indexer_db_password}"
INDEXER_RUNTIME_DIR="${INDEXER_RUNTIME_DIR:-/tmp/opennodia-indexer}"
INDEXER_DATA="${INDEXER_DATA:-${INDEXER_RUNTIME_DIR}/data}"

case "$INDEXER_DB_HOST" in
    ''|*[!A-Za-z0-9.-]*)
        echo "[indexer-wrapper] ERROR: invalid database host" >&2
        exit 1
        ;;
esac
case "$INDEXER_DB_PORT" in
    ''|*[!0-9]*)
        echo "[indexer-wrapper] ERROR: invalid database port" >&2
        exit 1
        ;;
esac
case "$INDEXER_DB_USER:$INDEXER_DB_NAME" in
    *[!A-Za-z0-9_:]*)
        echo "[indexer-wrapper] ERROR: invalid database identifier" >&2
        exit 1
        ;;
esac

database_password=$(read_secret_file "$INDEXER_DB_PASSWORD_FILE" "Indexer database password")
require_alphanumeric_secret "$database_password" "Indexer database password" 24 128

mkdir -p "$INDEXER_RUNTIME_DIR" "$INDEXER_DATA"
config_path="${INDEXER_RUNTIME_DIR}/indexer.yml"
umask 077
{
    printf 'postgres-connection-string: "host=%s port=%s user=%s password=%s dbname=%s sslmode=disable"\n' \
        "$INDEXER_DB_HOST" \
        "$INDEXER_DB_PORT" \
        "$INDEXER_DB_USER" \
        "$database_password" \
        "$INDEXER_DB_NAME"
    printf 'server: ":8980"\n'
} > "$config_path"
chown -R algorand:algorand "$INDEXER_RUNTIME_DIR"
chmod 600 "$config_path"

unset database_password
export INDEXER_DATA

/usr/local/bin/docker-entrypoint.sh \
    daemon \
    --configfile "$config_path" \
    --data-dir "$INDEXER_DATA" &
indexer_pid=$!

cleanup_config() {
    if [ -f "$config_path" ]; then
        config_size=$(stat -c '%s' "$config_path" 2>/dev/null || echo 0)
        case "$config_size" in
            ''|*[!0-9]*) config_size=0 ;;
        esac
        if [ "$config_size" -gt 0 ]; then
            dd if=/dev/zero of="$config_path" bs="$config_size" count=1 \
                conv=notrunc status=none 2>/dev/null || true
        fi
        rm -f "$config_path"
    fi
}

trap 'kill -TERM "$indexer_pid" 2>/dev/null || true' TERM INT

for _ in $(seq 1 60); do
    if ! kill -0 "$indexer_pid" 2>/dev/null; then
        break
    fi
    if perl /usr/local/bin/http-healthcheck.pl \
        127.0.0.1 8980 /health >/dev/null 2>&1; then
        cleanup_config
        break
    fi
    sleep 1
done

set +e
wait "$indexer_pid"
exit_code=$?
set -e
cleanup_config
exit "$exit_code"
