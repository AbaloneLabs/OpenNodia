#!/usr/bin/env bash
# Render Conduit configuration and resume from PostgreSQL/metadata state.

set -Eeuo pipefail

# shellcheck source=docker/secret-utils.sh
. /secret-utils.sh

FOLLOWER_HOST="${ALGOD_FOLLOWER_HOST:-algod-follower}"
FOLLOWER_PORT="${ALGOD_FOLLOWER_PORT:-8080}"
DATA_DIR="${CONDUIT_DATA_DIR:-/data}"
CONFIG_TEMPLATE="/etc/algorand/conduit.yml"
CONFIG_PATH="${DATA_DIR}/conduit.yml"
METADATA_PATH="${DATA_DIR}/metadata.json"
INDEXER_DB_HOST="${INDEXER_DB_HOST:-postgres}"
INDEXER_DB_PORT="${INDEXER_DB_PORT:-5432}"
ALGOD_TOKEN_FILE="${ALGOD_TOKEN_FILE:-/run/secrets/algod_token}"
INDEXER_DB_PASSWORD_FILE="${INDEXER_DB_PASSWORD_FILE:-/run/secrets/indexer_db_password}"

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

require_identifier() {
    local name="$1"
    local value="${!name:-}"

    if [[ -z "$value" || "$value" =~ [^A-Za-z0-9_] ]]; then
        echo "[conduit-wrapper] ERROR: ${name} contains invalid characters"
        exit 1
    fi
}

require_identifier INDEXER_DB_USER
require_identifier INDEXER_DB_NAME

if [[ -z "$INDEXER_DB_HOST" || "$INDEXER_DB_HOST" =~ [^A-Za-z0-9.-] ]]; then
    echo "[conduit-wrapper] ERROR: INDEXER_DB_HOST contains invalid characters"
    exit 1
fi
if [[ -z "$INDEXER_DB_PORT" || "$INDEXER_DB_PORT" =~ [^0-9] ]]; then
    echo "[conduit-wrapper] ERROR: INDEXER_DB_PORT must be numeric"
    exit 1
fi

export FOLLOWER_HOST FOLLOWER_PORT
export INDEXER_DB_USER INDEXER_DB_NAME
export INDEXER_DB_HOST INDEXER_DB_PORT

if [[ ! -r "$CONFIG_TEMPLATE" ]]; then
    echo "[conduit-wrapper] ERROR: missing configuration template: ${CONFIG_TEMPLATE}"
    exit 1
fi

chown -R algorand:algorand "$DATA_DIR"

CONFIG_TMP="${CONFIG_PATH}.tmp"
ALGOD_TOKEN="$ALGOD_TOKEN" \
INDEXER_DB_PASSWORD="$INDEXER_DB_PASSWORD" \
perl -pe '
    s/__ALGOD_TOKEN__/$ENV{ALGOD_TOKEN}/g;
    s/__INDEXER_DB_USER__/$ENV{INDEXER_DB_USER}/g;
    s/__INDEXER_DB_PASSWORD__/$ENV{INDEXER_DB_PASSWORD}/g;
    s/__INDEXER_DB_NAME__/$ENV{INDEXER_DB_NAME}/g;
    s/__INDEXER_DB_HOST__/$ENV{INDEXER_DB_HOST}/g;
    s/__INDEXER_DB_PORT__/$ENV{INDEXER_DB_PORT}/g;
' "$CONFIG_TEMPLATE" > "$CONFIG_TMP"
chmod 600 "$CONFIG_TMP"
chown algorand:algorand "$CONFIG_TMP"
mv "$CONFIG_TMP" "$CONFIG_PATH"
unset INDEXER_DB_PASSWORD

echo "[conduit-wrapper] waiting for follower node to be ready..."
READY_STREAK=0
for i in $(seq 1 300); do
    READY=$(ALGOD_TOKEN="$ALGOD_TOKEN" perl -e '
        use strict;
        use warnings;
        use IO::Socket::INET;

        my $socket = IO::Socket::INET->new(
            PeerAddr => $ENV{FOLLOWER_HOST},
            PeerPort => $ENV{FOLLOWER_PORT},
            Proto    => "tcp",
            Timeout  => 5,
        ) or exit 1;
        print {$socket} "GET /v2/status HTTP/1.0\r\n"
            . "Host: $ENV{FOLLOWER_HOST}:$ENV{FOLLOWER_PORT}\r\n"
            . "X-Algo-API-Token: $ENV{ALGOD_TOKEN}\r\n"
            . "Connection: close\r\n\r\n";

        my $response = "";
        while (my $line = <$socket>) {
            $response .= $line;
        }
        close $socket;

        my ($round) = $response =~ /"last-round":(\d+)/;
        my ($catchpoint) = $response =~ /"catchpoint":"([^"]*)"/;
        exit 1 unless defined $round && defined $catchpoint && $catchpoint eq "";

        my $delta_socket = IO::Socket::INET->new(
            PeerAddr => $ENV{FOLLOWER_HOST},
            PeerPort => $ENV{FOLLOWER_PORT},
            Proto    => "tcp",
            Timeout  => 5,
        ) or exit 1;
        my $ready_path = $round == 0
            ? "/v2/blocks/0?format=msgpack"
            : "/v2/deltas/$round?format=msgp";
        print {$delta_socket} "GET $ready_path HTTP/1.0\r\n"
            . "Host: $ENV{FOLLOWER_HOST}:$ENV{FOLLOWER_PORT}\r\n"
            . "X-Algo-API-Token: $ENV{ALGOD_TOKEN}\r\n"
            . "Connection: close\r\n\r\n";
        my $delta_status = <$delta_socket> // "";
        close $delta_socket;

        print $round if $delta_status =~ m{^HTTP/\d+(?:\.\d+)? 200(?:\s|$)};
    ' 2>/dev/null || true)

    if [[ -n "$READY" ]]; then
        READY_STREAK=$((READY_STREAK + 1))
        if [[ ! -s "$METADATA_PATH" && "$READY" -eq 0 ]]; then
            echo "[conduit-wrapper] fresh follower is ready at genesis"
            break
        fi
        if ((READY_STREAK >= 3)); then
            echo "[conduit-wrapper] follower is stable at round ${READY}"
            break
        fi
    else
        READY_STREAK=0
    fi
    if ((i % 15 == 0)); then
        echo "[conduit-wrapper] still waiting... (attempt ${i})"
    fi
    sleep 2
done

if [[ -z "${READY:-}" ]]; then
    echo "[conduit-wrapper] ERROR: follower node did not become ready in time"
    exit 1
fi
if [[ -s "$METADATA_PATH" && "$READY_STREAK" -lt 3 ]]; then
    echo "[conduit-wrapper] ERROR: follower node did not become ready in time"
    exit 1
fi

echo "[conduit-wrapper] starting from persisted PostgreSQL/metadata state"
unset ALGOD_TOKEN

set +e
gosu algorand conduit "$@" &
CONDUIT_PID=$!
set -e

cleanup_config() {
    if [[ -f "$CONFIG_PATH" ]]; then
        config_size=$(stat -c '%s' "$CONFIG_PATH" 2>/dev/null || echo 0)
        if [[ "$config_size" =~ ^[0-9]+$ ]] && ((config_size > 0)); then
            dd if=/dev/zero of="$CONFIG_PATH" bs="$config_size" count=1 \
                conv=notrunc status=none 2>/dev/null || true
        fi
        rm -f "$CONFIG_PATH"
    fi
}

trap 'kill -TERM "$CONDUIT_PID" 2>/dev/null || true' TERM INT

for _ in $(seq 1 60); do
    if ! kill -0 "$CONDUIT_PID" 2>/dev/null; then
        break
    fi
    if perl /usr/local/bin/http-healthcheck.pl \
        127.0.0.1 8981 /health >/dev/null 2>&1; then
        cleanup_config
        break
    fi
    sleep 1
done

set +e
wait "$CONDUIT_PID"
exit_code=$?
set -e
cleanup_config
exit "$exit_code"
