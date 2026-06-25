#!/bin/sh
# Generate Rust and npm SBOMs for release artifacts.

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "${SCRIPT_DIR}/../.." && pwd)
OUT_DIR="${1:-${REPO_ROOT}/dist/sbom}"
case "$OUT_DIR" in
    /*) ;;
    *) OUT_DIR="${REPO_ROOT}/${OUT_DIR}" ;;
esac

mkdir -p "$OUT_DIR"
cd "$REPO_ROOT"

if ! cargo cyclonedx --version >/dev/null 2>&1; then
    echo "ERROR: cargo-cyclonedx is required. Install with: cargo install cargo-cyclonedx" >&2
    exit 1
fi
(
    cd "$OUT_DIR"
    cargo cyclonedx --manifest-path "$REPO_ROOT/Cargo.toml" --format xml --override-filename cargo-cyclonedx
)
find "$REPO_ROOT/crates" -mindepth 2 -maxdepth 2 -name cargo-cyclonedx.xml -print | while IFS= read -r sbom; do
    crate_name=$(basename "$(dirname "$sbom")")
    mv "$sbom" "$OUT_DIR/${crate_name}-cyclonedx.xml"
done
if [ ! -f "$OUT_DIR/opennodia-server-cyclonedx.xml" ]; then
    echo "ERROR: cargo-cyclonedx did not generate expected crate SBOMs" >&2
    exit 1
fi

cd "$REPO_ROOT/frontend"
npm sbom --sbom-format cyclonedx --json > "$OUT_DIR/npm-cyclonedx.json"

echo "SBOMs written to $OUT_DIR"
