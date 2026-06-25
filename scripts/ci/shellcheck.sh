#!/bin/sh
# Run shellcheck against tracked shell scripts, using Docker when needed.

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "${SCRIPT_DIR}/../.." && pwd)
SHELLCHECK_IMAGE="${SHELLCHECK_IMAGE:-koalaman/shellcheck:stable}"

cd "$REPO_ROOT"

file_list=$(mktemp)
trap 'rm -f "$file_list"' EXIT HUP INT TERM
git ls-files -z \
    '*.sh' \
    'docker/*.sh' \
    'scripts/*.sh' \
    'scripts/**/*.sh' |
    sort -zu > "$file_list"

if [ ! -s "$file_list" ]; then
    echo "No tracked shell scripts found."
    exit 0
fi

if command -v shellcheck >/dev/null 2>&1; then
    xargs -0 shellcheck -x < "$file_list"
else
    xargs -0 docker run --rm \
        -v "$REPO_ROOT:/mnt:ro" \
        -w /mnt \
        "$SHELLCHECK_IMAGE" \
        -x < "$file_list"
fi
