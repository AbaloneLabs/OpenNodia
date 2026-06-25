#!/bin/sh
# Generate SHA-256 checksums for release files.

set -eu

TARGET_DIR="${1:-dist/release}"

if [ ! -d "$TARGET_DIR" ]; then
    echo "ERROR: target directory does not exist: $TARGET_DIR" >&2
    exit 1
fi

(
    cd "$TARGET_DIR"
    tmp_file=$(mktemp SHA256SUMS.XXXXXX)
    trap 'rm -f "$tmp_file"' EXIT HUP INT TERM
    find . -type f ! -name SHA256SUMS -print0 |
        sort -z |
        xargs -0 -r sha256sum > "$tmp_file"
    mv "$tmp_file" SHA256SUMS
    trap - EXIT HUP INT TERM
)

echo "Wrote ${TARGET_DIR}/SHA256SUMS"
