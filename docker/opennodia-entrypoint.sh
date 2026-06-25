#!/bin/sh
# Copy host-owned secret mounts into an ephemeral non-root runtime directory.

set -eu

# shellcheck source=docker/secret-utils.sh
. /secret-utils.sh

runtime_dir="${OPENNODIA_RUNTIME_SECRET_DIR:-/tmp/opennodia-secrets}"
algod_source="${OPENNODIA_ALGOD_TOKEN_FILE:-/run/secrets/algod_token}"
database_source="${OPENNODIA_WALLET_HISTORY_DATABASE_PASSWORD_FILE:-/run/secrets/indexer_db_password}"

algod_token=$(read_secret_file "$algod_source" "algod API token")
database_password=$(read_secret_file "$database_source" "Indexer database password")
require_alphanumeric_secret "$algod_token" "algod API token" 64 64
require_alphanumeric_secret \
    "$database_password" \
    "Indexer database password" \
    24 \
    128

umask 077
mkdir -p "$runtime_dir"
printf '%s\n' "$algod_token" > "${runtime_dir}/algod.token"
printf '%s\n' "$database_password" > "${runtime_dir}/indexer-db-password"
unset algod_token database_password

chown -R opennodia:opennodia "$runtime_dir"
chmod 700 "$runtime_dir"
chmod 400 "${runtime_dir}/algod.token" "${runtime_dir}/indexer-db-password"

export OPENNODIA_ALGOD_TOKEN_FILE="${runtime_dir}/algod.token"
export OPENNODIA_WALLET_HISTORY_DATABASE_PASSWORD_FILE="${runtime_dir}/indexer-db-password"

setpriv \
    --reuid=opennodia \
    --regid=opennodia \
    --init-groups \
    "$@" &
server_pid=$!

cleanup_secrets() {
    for secret_path in \
        "${runtime_dir}/algod.token" \
        "${runtime_dir}/indexer-db-password"; do
        if [ -f "$secret_path" ]; then
            secret_size=$(stat -c '%s' "$secret_path" 2>/dev/null || echo 0)
            case "$secret_size" in
                ''|*[!0-9]*) secret_size=0 ;;
            esac
            if [ "$secret_size" -gt 0 ]; then
                dd if=/dev/zero of="$secret_path" bs="$secret_size" count=1 \
                    conv=notrunc status=none 2>/dev/null || true
            fi
            rm -f "$secret_path"
        fi
    done
}

trap 'kill -TERM "$server_pid" 2>/dev/null || true' TERM INT

for _ in $(seq 1 10); do
    if ! kill -0 "$server_pid" 2>/dev/null; then
        break
    fi
    sleep 1
done
cleanup_secrets

set +e
wait "$server_pid"
exit_code=$?
set -e
cleanup_secrets
exit "$exit_code"
