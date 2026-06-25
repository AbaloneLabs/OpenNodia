#!/bin/sh
# Shared helpers for reading file-based runtime secrets without printing them.

read_secret_file() {
    secret_path="$1"
    secret_label="$2"

    if [ ! -r "$secret_path" ]; then
        echo "[secrets] ERROR: ${secret_label} file is not readable: ${secret_path}" >&2
        return 1
    fi

    secret_value=$(tr -d '\r\n' < "$secret_path")
    if [ -z "$secret_value" ]; then
        echo "[secrets] ERROR: ${secret_label} file is empty" >&2
        return 1
    fi

    printf '%s' "$secret_value"
}

require_alphanumeric_secret() {
    secret_value="$1"
    secret_label="$2"
    minimum_length="$3"
    maximum_length="$4"

    case "$secret_value" in
        *[!A-Za-z0-9_]*)
            echo "[secrets] ERROR: ${secret_label} must be alphanumeric" >&2
            return 1
            ;;
    esac

    secret_length=${#secret_value}
    if [ "$secret_length" -lt "$minimum_length" ] ||
        [ "$secret_length" -gt "$maximum_length" ]; then
        echo "[secrets] ERROR: ${secret_label} has an invalid length" >&2
        return 1
    fi
}

write_pgpass_file() {
    pgpass_path="$1"
    pg_host="$2"
    pg_port="$3"
    pg_database="$4"
    pg_user="$5"
    pg_password="$6"

    umask 077
    printf '%s:%s:%s:%s:%s\n' \
        "$pg_host" "$pg_port" "$pg_database" "$pg_user" "$pg_password" \
        > "$pgpass_path"
    chmod 600 "$pgpass_path"
}
