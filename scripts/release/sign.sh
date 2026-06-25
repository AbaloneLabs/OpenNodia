#!/bin/sh
# Sign release checksums and optionally a container image with cosign.

set -eu

TARGET_DIR="${1:-dist/release}"
IMAGE_REF="${OPENNODIA_IMAGE_REF:-}"

if ! command -v cosign >/dev/null 2>&1; then
    echo "ERROR: cosign is required for public release signing" >&2
    exit 2
fi

if [ ! -f "${TARGET_DIR}/SHA256SUMS" ]; then
    echo "ERROR: missing ${TARGET_DIR}/SHA256SUMS; run checksums.sh first" >&2
    exit 1
fi

cosign sign-blob \
    --yes \
    --output-signature "${TARGET_DIR}/SHA256SUMS.sig" \
    --output-certificate "${TARGET_DIR}/SHA256SUMS.pem" \
    "${TARGET_DIR}/SHA256SUMS"

if [ -n "$IMAGE_REF" ]; then
    cosign sign --yes "$IMAGE_REF"
fi

echo "Signed ${TARGET_DIR}/SHA256SUMS"
