#!/bin/sh
# Fail when known installation secrets or obvious credentials enter Git.

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "${SCRIPT_DIR}/.." && pwd)
ENV_FILE="${OPENNODIA_ENV_FILE:-${REPO_ROOT}/.env}"

read_env_value() {
    env_key="$1"
    env_path="$2"
    [ -f "$env_path" ] || return 0
    sed -n "s/^${env_key}=//p" "$env_path" | tail -n 1
}

secret_dir="${OPENNODIA_SECRETS_DIR:-$(read_env_value OPENNODIA_SECRETS_DIR "$ENV_FILE")}"
staged_diff=$(mktemp)
trap 'rm -f "$staged_diff"' EXIT HUP INT TERM
git -C "$REPO_ROOT" diff --cached --no-ext-diff --text > "$staged_diff"

check_secret_file() {
    secret_path="$1"
    secret_label="$2"
    [ -s "$secret_path" ] || return 0

    secret_value=$(tr -d '\r\n' < "$secret_path")
    if git -C "$REPO_ROOT" grep -F -q -- "$secret_value" -- . ||
        grep -F -q -- "$secret_value" "$staged_diff"; then
        echo "ERROR: ${secret_label} appears in Git-tracked content" >&2
        unset secret_value
        exit 1
    fi
    unset secret_value
}

if [ -n "$secret_dir" ]; then
    check_secret_file "${secret_dir}/algod.token" "algod token"
    check_secret_file "${secret_dir}/indexer-db-password" "Indexer database password"
fi

if grep -E -q \
    '^\+[^+].*(ALGOD_TOKEN|INDEXER_DB_PASSWORD)=[A-Za-z0-9_]{16,}' \
    "$staged_diff"; then
    echo "ERROR: a plaintext OpenNodia credential is staged" >&2
    exit 1
fi

if grep -E -q \
    '^\+[^+].*(postgres(ql)?://[^[:space:]\"'\"']+:[^[:space:]\"'\"'@]+@|BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY)' \
    "$staged_diff"; then
    echo "ERROR: a likely credential or private key is staged" >&2
    exit 1
fi

echo "Secret scan passed."
