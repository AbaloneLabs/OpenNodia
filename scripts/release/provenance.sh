#!/bin/sh
# Write minimal local provenance metadata for release artifacts.

set -eu

TARGET_DIR="${1:-dist/release}"
mkdir -p "$TARGET_DIR"

commit=$(git rev-parse HEAD)
dirty=$(git status --short)
timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
image_digest="${OPENNODIA_IMAGE_DIGEST:-}"
if [ -z "$image_digest" ] && [ -f "${TARGET_DIR}/opennodia-image.tar" ]; then
    image_digest="sha256:$(sha256sum "${TARGET_DIR}/opennodia-image.tar" | awk '{print $1}')"
fi

if [ -n "$dirty" ]; then
    dirty_json=true
else
    dirty_json=false
fi

cat > "${TARGET_DIR}/provenance.json" <<EOF
{
  "project": "OpenNodia",
  "git_commit": "${commit}",
  "git_dirty": ${dirty_json},
  "built_at": "${timestamp}",
  "image_digest": "${image_digest}",
  "checksums": "SHA256SUMS",
  "sbom": [
    "opennodia-*-cyclonedx.xml",
    "npm-cyclonedx.json"
  ]
}
EOF

echo "Wrote ${TARGET_DIR}/provenance.json"
