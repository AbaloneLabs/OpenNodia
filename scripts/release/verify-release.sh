#!/bin/sh
# Verify release artifacts are internally reproducible and, for public release,
# signed.

set -eu

TARGET_DIR="${1:-dist/release}"
REQUIRE_SIGNATURES="${OPENNODIA_REQUIRE_SIGNATURES:-false}"
CERTIFICATE_IDENTITY="${OPENNODIA_COSIGN_CERTIFICATE_IDENTITY:-}"
CERTIFICATE_OIDC_ISSUER="${OPENNODIA_COSIGN_CERTIFICATE_OIDC_ISSUER:-}"
IMAGE_REF="${OPENNODIA_IMAGE_REF:-}"

if [ ! -d "$TARGET_DIR" ]; then
    echo "ERROR: target directory does not exist: $TARGET_DIR" >&2
    exit 1
fi

if [ ! -f "${TARGET_DIR}/SHA256SUMS" ]; then
    echo "ERROR: missing SHA256SUMS" >&2
    exit 1
fi

(
    cd "$TARGET_DIR"
    sha256sum -c SHA256SUMS
)

if [ ! -f "${TARGET_DIR}/provenance.json" ]; then
    echo "ERROR: missing provenance.json" >&2
    exit 1
fi

if ! jq -e '.project == "OpenNodia" and (.git_commit | type == "string") and (.checksums == "SHA256SUMS")' "${TARGET_DIR}/provenance.json" >/dev/null; then
    echo "ERROR: provenance.json is missing required release metadata" >&2
    exit 1
fi

if [ "$REQUIRE_SIGNATURES" = "true" ]; then
    if [ ! -f "${TARGET_DIR}/SHA256SUMS.sig" ] || [ ! -f "${TARGET_DIR}/SHA256SUMS.pem" ]; then
        echo "ERROR: release signatures are required but SHA256SUMS signature/certificate is missing" >&2
        exit 1
    fi
    if ! command -v cosign >/dev/null 2>&1; then
        echo "ERROR: cosign is required to verify public release signatures" >&2
        exit 2
    fi
    if [ -z "$CERTIFICATE_IDENTITY" ] || [ -z "$CERTIFICATE_OIDC_ISSUER" ]; then
        echo "ERROR: set OPENNODIA_COSIGN_CERTIFICATE_IDENTITY and OPENNODIA_COSIGN_CERTIFICATE_OIDC_ISSUER to verify signatures" >&2
        exit 1
    fi
    cosign verify-blob \
        --certificate "${TARGET_DIR}/SHA256SUMS.pem" \
        --signature "${TARGET_DIR}/SHA256SUMS.sig" \
        --certificate-identity "$CERTIFICATE_IDENTITY" \
        --certificate-oidc-issuer "$CERTIFICATE_OIDC_ISSUER" \
        "${TARGET_DIR}/SHA256SUMS"
    if [ -n "$IMAGE_REF" ]; then
        cosign verify \
            --certificate-identity "$CERTIFICATE_IDENTITY" \
            --certificate-oidc-issuer "$CERTIFICATE_OIDC_ISSUER" \
            "$IMAGE_REF" >/dev/null
    fi
fi

echo "Release artifact verification passed"
