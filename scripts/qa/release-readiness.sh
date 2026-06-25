#!/bin/sh
# Report missing release-blocking evidence before a public release.

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "${SCRIPT_DIR}/../.." && pwd)
CALLER_CWD=$(pwd)
if [ "$#" -ge 1 ]; then
    case "$1" in
        /*) TESTNET_EVIDENCE="$1" ;;
        *) TESTNET_EVIDENCE="${CALLER_CWD}/$1" ;;
    esac
else
    TESTNET_EVIDENCE="${REPO_ROOT}/scripts/qa/testnet-evidence.json"
fi
if [ "$#" -ge 2 ]; then
    case "$2" in
        /*) PUBLIC_EVIDENCE="$2" ;;
        *) PUBLIC_EVIDENCE="${CALLER_CWD}/$2" ;;
    esac
else
    PUBLIC_EVIDENCE="${REPO_ROOT}/scripts/qa/public-release-evidence.json"
fi
STRICT="${OPENNODIA_RELEASE_READINESS_STRICT:-false}"
missing=0

cd "$REPO_ROOT"

note() {
    printf '%s\n' "$1"
}

missing_item() {
    missing=$((missing + 1))
    printf 'MISSING: %s\n' "$1" >&2
}

require_jq() {
    if ! command -v jq >/dev/null 2>&1; then
        echo "ERROR: jq is required for release readiness checks" >&2
        exit 2
    fi
}

require_json_query() {
    file="$1"
    query="$2"
    label="$3"
    if ! jq -e "$query" "$file" >/dev/null; then
        missing_item "$label"
    fi
}

check_testnet_evidence() {
    if [ ! -f "$TESTNET_EVIDENCE" ]; then
        missing_item "Testnet evidence file is missing: $TESTNET_EVIDENCE"
        return
    fi
    if ! jq empty "$TESTNET_EVIDENCE" >/dev/null; then
        missing_item "Testnet evidence file is not valid JSON: $TESTNET_EVIDENCE"
        return
    fi

    require_json_query "$TESTNET_EVIDENCE" '.network == "testnet"' \
        "Testnet evidence network must be testnet"
    require_json_query "$TESTNET_EVIDENCE" '.signed_e2e.orders | length >= 2' \
        "Signed order create/fill/cancel evidence is required"
    require_json_query "$TESTNET_EVIDENCE" '.external_liquidity.route_candidates.candidate_count >= 1' \
        "Route candidate evidence is required"
    require_json_query "$TESTNET_EVIDENCE" '.external_liquidity.tinyman_swap.tx_id and .external_liquidity.tinyman_swap.confirmed_round and ((.external_liquidity.tinyman_swap.confirmed_amount_out // .external_liquidity.tinyman_swap.asset_out_balance_after) >= .external_liquidity.tinyman_swap.prepared_minimum_out)' \
        "Tinyman live swap evidence is required"
    require_json_query "$TESTNET_EVIDENCE" '.external_liquidity.tinyman_add_liquidity.tx_id and .external_liquidity.tinyman_add_liquidity.confirmed_round and (.external_liquidity.tinyman_add_liquidity.minted_lp >= .external_liquidity.tinyman_add_liquidity.minimum_lp)' \
        "Tinyman live LP add evidence is required"
    require_json_query "$TESTNET_EVIDENCE" '.external_liquidity.tinyman_remove_liquidity.tx_id and .external_liquidity.tinyman_remove_liquidity.confirmed_round and (.external_liquidity.tinyman_remove_liquidity.amount_0 >= .external_liquidity.tinyman_remove_liquidity.minimum_0) and (.external_liquidity.tinyman_remove_liquidity.amount_1 >= .external_liquidity.tinyman_remove_liquidity.minimum_1)' \
        "Tinyman live LP remove evidence is required"
    require_json_query "$TESTNET_EVIDENCE" '.external_liquidity.pact_discovery.factory_app_id and (.external_liquidity.pact_discovery.factory_box_count >= 1) and (.external_liquidity.pact_discovery.tradable_pool_count >= 1)' \
        "Pact factory discovery evidence is required"
    require_json_query "$TESTNET_EVIDENCE" '.external_liquidity.pact_swap.tx_id and .external_liquidity.pact_swap.confirmed_round and (.external_liquidity.pact_swap.confirmed_amount_out >= .external_liquidity.pact_swap.prepared_minimum_out)' \
        "Pact live swap evidence is required"
    require_json_query "$TESTNET_EVIDENCE" '.external_liquidity.pact_add_liquidity.tx_id and .external_liquidity.pact_add_liquidity.confirmed_round and (.external_liquidity.pact_add_liquidity.minted_lp >= .external_liquidity.pact_add_liquidity.minimum_lp)' \
        "Pact live LP add evidence is required"
    require_json_query "$TESTNET_EVIDENCE" '.external_liquidity.pact_remove_liquidity.tx_id and .external_liquidity.pact_remove_liquidity.confirmed_round and (.external_liquidity.pact_remove_liquidity.amount_0 >= .external_liquidity.pact_remove_liquidity.minimum_0) and (.external_liquidity.pact_remove_liquidity.amount_1 >= .external_liquidity.pact_remove_liquidity.minimum_1)' \
        "Pact live LP remove evidence is required"
}

check_public_release_evidence() {
    if [ ! -f "$PUBLIC_EVIDENCE" ]; then
        missing_item "Public release evidence file is missing: $PUBLIC_EVIDENCE"
        return
    fi
    if ! jq empty "$PUBLIC_EVIDENCE" >/dev/null; then
        missing_item "Public release evidence file is not valid JSON: $PUBLIC_EVIDENCE"
        return
    fi
    if ! python3 scripts/qa/public_release_evidence.py validate "$PUBLIC_EVIDENCE" >/dev/null; then
        missing_item "Public release evidence schema validation must pass"
        return
    fi

    require_json_query "$PUBLIC_EVIDENCE" '.cross_platform.macos.install_upgrade_reboot == "passed" and .cross_platform.macos.secret_reuse == "passed" and ((.cross_platform.macos.run_at_utc // "") | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) and ((.cross_platform.macos.os // "") | type == "string" and length > 0) and ((.cross_platform.macos.run_url // "") | test("^https://[^\\s]+$"))' \
        "macOS install/upgrade/reboot and secret reuse evidence is required"
    require_json_query "$PUBLIC_EVIDENCE" '.cross_platform.windows.install_upgrade_reboot == "passed" and .cross_platform.windows.secret_reuse == "passed" and ((.cross_platform.windows.run_at_utc // "") | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) and ((.cross_platform.windows.os // "") | type == "string" and length > 0) and ((.cross_platform.windows.run_url // "") | test("^https://[^\\s]+$"))' \
        "Windows install/upgrade/reboot and secret reuse evidence is required"
    require_json_query "$PUBLIC_EVIDENCE" '.cross_platform.linux_arm64.full_stack_sync_restart == "passed" and .cross_platform.linux_arm64.architecture == "arm64" and ((.cross_platform.linux_arm64.run_at_utc // "") | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) and ((.cross_platform.linux_arm64.os // "") | type == "string" and length > 0) and ((.cross_platform.linux_arm64.docker_version // "") | type == "string" and length > 0) and ((.cross_platform.linux_arm64.compose_version // "") | type == "string" and length > 0) and ((.cross_platform.linux_arm64.run_url // "") | test("^https://[^\\s]+$"))' \
        "Linux ARM64 full-stack sync/restart evidence is required"
    require_json_query "$PUBLIC_EVIDENCE" '((.release_artifact.tag // "") | test("^v[0-9A-Za-z._-]+$")) and ((.release_artifact.workflow_run_url // "") | test("^https://[^\\s]+$")) and ((.release_artifact.git_commit // "") | test("^[0-9a-f]{40}$")) and ((.release_artifact.image_digest // "") | test("^sha256:[0-9a-f]{64}$")) and .release_artifact.checksums_verified == true and .release_artifact.provenance_verified == true and .release_artifact.signatures_verified == true and .release_artifact.licenses_verified == true' \
        "Signed release artifact checksum/provenance/license evidence is required"
    require_json_query "$PUBLIC_EVIDENCE" '.external_review.status == "passed" and ((.external_review.reviewer // "") | type == "string" and length > 0) and ((.external_review.completed_at_utc // "") | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) and ((.external_review.report_url // "") | test("^https://[^\\s]+$")) and .external_review.blockers_open == 0' \
        "External security review must pass with zero open blockers"
    require_json_query "$PUBLIC_EVIDENCE" '.upgrade_rollback.clean_install == "passed" and .upgrade_rollback.upgrade == "passed" and .upgrade_rollback.rollback == "passed" and .upgrade_rollback.testnet_validation == "passed" and ((.upgrade_rollback.run_at_utc // "") | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) and ((.upgrade_rollback.source_version // "") | type == "string" and length > 0) and ((.upgrade_rollback.target_version // "") | type == "string" and length > 0) and ((.upgrade_rollback.run_url // "") | test("^https://[^\\s]+$"))' \
        "Clean install, upgrade, rollback, and Testnet validation evidence is required"
}

require_jq
check_testnet_evidence
check_public_release_evidence

if [ "$missing" -eq 0 ]; then
    note "Release readiness evidence is complete."
    exit 0
fi

note "Release readiness evidence is incomplete: $missing missing item(s)."
if [ "$STRICT" = "true" ]; then
    exit 1
fi
exit 0
