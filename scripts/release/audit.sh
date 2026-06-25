#!/bin/sh
# Run dependency and container audits for release candidates.

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "${SCRIPT_DIR}/../.." && pwd)

cd "$REPO_ROOT"

if ! command -v cargo-audit >/dev/null 2>&1; then
    echo "ERROR: cargo-audit is required. Install with: cargo install cargo-audit" >&2
    exit 1
fi
cargo audit --deny warnings

cd "$REPO_ROOT/frontend"
npm audit --audit-level=moderate

cd "$REPO_ROOT"
"$REPO_ROOT/scripts/ci/trivy-config.sh"
