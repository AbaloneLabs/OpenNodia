#!/bin/sh
# Run Trivy configuration scanning, using Docker when needed.

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "${SCRIPT_DIR}/../.." && pwd)
TRIVY_IMAGE="${TRIVY_IMAGE:-aquasec/trivy:latest}"
TRIVY_CACHE_DIR="${TRIVY_CACHE_DIR:-${HOME:-/tmp}/.cache/trivy}"

mkdir -p "$TRIVY_CACHE_DIR"
cd "$REPO_ROOT"

run_trivy_config() {
    "$@" config \
        --exit-code 1 \
        --severity HIGH,CRITICAL \
        --timeout 10m \
        --skip-dirs frontend/node_modules \
        --skip-dirs target \
        --skip-dirs dist \
        --skip-dirs opensource \
        .
}

if command -v trivy >/dev/null 2>&1; then
    run_trivy_config trivy
else
    run_trivy_config docker run --rm \
        -v "$REPO_ROOT:/project:ro" \
        -v "$TRIVY_CACHE_DIR:/root/.cache/trivy" \
        -w /project \
        "$TRIVY_IMAGE"
fi
