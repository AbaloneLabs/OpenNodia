#!/bin/sh
# Generate installation-specific secrets outside the Git working tree.

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "${SCRIPT_DIR}/.." && pwd)
ENV_FILE="${OPENNODIA_ENV_FILE:-${REPO_ROOT}/.env}"
ENV_EXAMPLE="${REPO_ROOT}/.env.example"

read_env_value() {
    env_key="$1"
    env_path="$2"
    [ -f "$env_path" ] || return 0
    sed -n "s/^${env_key}=//p" "$env_path" | tail -n 1
}

generate_hex() {
    byte_count="$1"
    if command -v openssl >/dev/null 2>&1; then
        openssl rand -hex "$byte_count"
        return
    fi
    od -An -N "$byte_count" -tx1 /dev/urandom | tr -d ' \n'
}

is_private_ipv4() {
    printf '%s\n' "$1" | awk -F. '
        NF != 4 { exit 1 }
        {
            for (i = 1; i <= 4; i++) {
                if ($i !~ /^[0-9]+$/ || $i < 0 || $i > 255) {
                    exit 1
                }
            }
            if ($1 == 10 ||
                ($1 == 172 && $2 >= 16 && $2 <= 31) ||
                ($1 == 192 && $2 == 168)) {
                exit 0
            }
            exit 1
        }
    '
}

detect_private_lan_ipv4() {
    candidate=''

    if command -v ip >/dev/null 2>&1; then
        candidate=$(ip -4 route get 1.1.1.1 2>/dev/null |
            awk '{
                for (i = 1; i <= NF; i++) {
                    if ($i == "src" && (i + 1) <= NF) {
                        print $(i + 1)
                        exit
                    }
                }
            }')
        if [ -n "$candidate" ] && is_private_ipv4 "$candidate"; then
            printf '%s\n' "$candidate"
            return 0
        fi
    fi

    if command -v route >/dev/null 2>&1 && command -v ipconfig >/dev/null 2>&1; then
        interface=$(route -n get default 2>/dev/null |
            awk '/interface:/{print $2; exit}')
        if [ -n "$interface" ]; then
            candidate=$(ipconfig getifaddr "$interface" 2>/dev/null || true)
            if [ -n "$candidate" ] && is_private_ipv4 "$candidate"; then
                printf '%s\n' "$candidate"
                return 0
            fi
        fi
    fi

    if command -v hostname >/dev/null 2>&1; then
        for candidate in $(hostname -I 2>/dev/null || true); do
            if is_private_ipv4 "$candidate"; then
                printf '%s\n' "$candidate"
                return 0
            fi
        done
    fi

    if command -v ifconfig >/dev/null 2>&1; then
        for candidate in $(ifconfig 2>/dev/null |
            awk '/inet /{
                for (i = 1; i <= NF; i++) {
                    if ($i == "inet") {
                        print $(i + 1)
                    }
                }
            }'); do
            if is_private_ipv4 "$candidate"; then
                printf '%s\n' "$candidate"
                return 0
            fi
        done
    fi

    return 1
}

write_secret_if_missing() {
    secret_path="$1"
    legacy_value="$2"
    byte_count="$3"
    minimum_length="$4"

    if [ ! -s "$secret_path" ]; then
        secret_value="$legacy_value"
        case "$secret_value" in
            ''|replace-with-*)
                secret_value=$(generate_hex "$byte_count")
                ;;
        esac
        case "$secret_value" in
            *[!A-Za-z0-9_]*)
                echo "ERROR: legacy secret contains unsupported characters" >&2
                exit 1
                ;;
        esac
        if [ "${#secret_value}" -lt "$minimum_length" ]; then
            echo "ERROR: legacy secret is too short" >&2
            exit 1
        fi
        umask 077
        printf '%s\n' "$secret_value" > "$secret_path"
        unset secret_value
    fi
    chmod 600 "$secret_path"
}

existing_secret_dir=$(read_env_value OPENNODIA_SECRETS_DIR "$ENV_FILE")
if [ -n "${OPENNODIA_SECRETS_DIR:-}" ]; then
    secret_dir="$OPENNODIA_SECRETS_DIR"
elif [ -n "$existing_secret_dir" ]; then
    secret_dir="$existing_secret_dir"
else
    config_home="${XDG_CONFIG_HOME:-${HOME:?HOME is required}/.config}"
    secret_dir="${config_home}/opennodia/secrets"
fi

case "$secret_dir" in
    /*) ;;
    *)
        echo "ERROR: OPENNODIA_SECRETS_DIR must be an absolute path" >&2
        exit 1
        ;;
esac

legacy_algod_token=$(read_env_value ALGOD_TOKEN "$ENV_FILE")
legacy_database_password=$(read_env_value INDEXER_DB_PASSWORD "$ENV_FILE")
existing_bind_address=$(read_env_value OPENNODIA_BIND_ADDRESS "$ENV_FILE")

if [ -n "${OPENNODIA_BIND_ADDRESS:-}" ]; then
    bind_address="$OPENNODIA_BIND_ADDRESS"
elif [ -n "$existing_bind_address" ]; then
    bind_address="$existing_bind_address"
elif bind_address=$(detect_private_lan_ipv4); then
    :
else
    bind_address="127.0.0.1"
    echo "WARNING: no private LAN IPv4 address detected; using loopback." >&2
fi

umask 077
mkdir -p "$secret_dir"
chmod 700 "$secret_dir"
write_secret_if_missing "${secret_dir}/algod.token" "$legacy_algod_token" 32 64
write_secret_if_missing \
    "${secret_dir}/indexer-db-password" \
    "$legacy_database_password" \
    24 \
    24
unset legacy_algod_token legacy_database_password

if [ ! -f "$ENV_FILE" ]; then
    cp "$ENV_EXAMPLE" "$ENV_FILE"
fi

env_tmp="${ENV_FILE}.tmp.$$"
awk '
    !/^(ALGOD_TOKEN|INDEXER_DB_PASSWORD|OPENNODIA_SECRETS_DIR|OPENNODIA_BIND_ADDRESS)=/
' "$ENV_FILE" > "$env_tmp"
printf '\nOPENNODIA_BIND_ADDRESS=%s\n' "$bind_address" >> "$env_tmp"
printf 'OPENNODIA_SECRETS_DIR=%s\n' "$secret_dir" >> "$env_tmp"
chmod 600 "$env_tmp"
mv "$env_tmp" "$ENV_FILE"

if git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    git -C "$REPO_ROOT" config core.hooksPath .githooks
fi

echo "OpenNodia secrets initialized outside the repository."
echo "Secret directory: ${secret_dir}"
echo "Docker Compose environment: ${ENV_FILE}"
echo "Web UI: http://${bind_address}:${OPENNODIA_HOST_PORT:-30080}"
