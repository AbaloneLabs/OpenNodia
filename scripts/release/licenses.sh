#!/bin/sh
# Generate dependency license snapshots for release review.

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "${SCRIPT_DIR}/../.." && pwd)
OUT_DIR="${1:-${REPO_ROOT}/dist/licenses}"
case "$OUT_DIR" in
    /*) ;;
    *) OUT_DIR="${REPO_ROOT}/${OUT_DIR}" ;;
esac

mkdir -p "$OUT_DIR"

cd "$REPO_ROOT"
cargo metadata --format-version 1 > "$OUT_DIR/cargo-metadata.json"

cd "$REPO_ROOT/frontend"
npm ls --all --json > "$OUT_DIR/npm-tree.json"
# shellcheck disable=SC2016
node -e '
const lock = require("./package-lock.json");
const rows = Object.entries(lock.packages || {})
  .filter(([path]) => path)
  .map(([path, pkg]) => ({
    path,
    name: pkg.name || path.replace(/^node_modules\//, ""),
    version: pkg.version || "",
    license: pkg.license || "UNKNOWN"
  }))
  .sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`));
process.stdout.write(JSON.stringify(rows, null, 2));
process.stdout.write("\n");
' > "$OUT_DIR/npm-licenses.json"

echo "License snapshots written to $OUT_DIR"
