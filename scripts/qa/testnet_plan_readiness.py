#!/usr/bin/env python3
"""Report missing Testnet evidence for the active DEX routing plans."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_EVIDENCE = REPO_ROOT / "scripts" / "qa" / "testnet-evidence.json"
ISO_UTC = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")

UNIFIED_ROUTING_CASES = {
    "orderbook_only_pair": "Unified routing orderbook-only pair",
    "native_pool_only_pair": "Unified routing native-pool-only pair",
    "tinyman_pact_pool_only_pair": "Unified routing Tinyman/Pact-only pair",
    "orderbook_amm_price_cross_pair": "Unified routing orderbook/AMM price-cross pair",
    "network_fee_changes_best_route": "Unified routing network-fee best-route change",
    "limit_partial_then_standing_remainder": "Unified routing limit partial fill plus standing remainder",
    "quote_then_state_change_rejected": "Unified routing quote/state-change rejection",
    "pair_reversal_decimals": "Unified routing pair reversal with differing decimals",
    "duplicate_folks_backed_pool": "Unified routing Folks-backed duplicate pool check",
}

DEX_ADDITIONAL_CASES = {
    "two_wallet_create_fill_cancel": "DEX two-wallet create/fill/cancel",
    "algo_asa_bid_ask_reverse": "DEX ALGO/ASA bid/ask and reverse view",
    "asa_algo_bid_ask_reverse": "DEX ASA/ALGO bid/ask and reverse view",
    "asa_asa_bid_ask_reverse": "DEX ASA/ASA bid/ask and reverse view",
    "ioc_no_match_balance": "DEX IOC no-match on-chain balance check",
    "ioc_single_fill_balance": "DEX IOC single-fill on-chain balance check",
    "ioc_multi_fill_balance": "DEX IOC multi-fill on-chain balance check",
    "ioc_discarded_remainder_balance": "DEX IOC discarded-remainder balance check",
    "expiry_boundary": "DEX expiry boundary",
    "duplicate_submit": "DEX duplicate submit",
    "intent_reuse": "DEX intent reuse",
    "server_restart": "DEX server restart",
    "stale_node_fallback": "DEX stale-node fallback",
    "artifact_secret_scan": "DEX evidence artifact secret scan",
}
SECTION_CASES = {
    "unified_routing": UNIFIED_ROUTING_CASES,
    "dex_additional_matrix": DEX_ADDITIONAL_CASES,
}


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise SystemExit(f"Testnet evidence file is missing: {path}") from None
    except json.JSONDecodeError as error:
        raise SystemExit(f"Testnet evidence file is not valid JSON: {path}: {error}") from None


def is_non_empty_string(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def is_positive_int(value: Any) -> bool:
    return isinstance(value, int) and value > 0


def is_tx_id(value: Any) -> bool:
    return isinstance(value, str) and bool(re.fullmatch(r"[A-Z2-7]{52}", value))


def get_path(data: Any, dotted_path: str) -> Any:
    current = data
    for part in dotted_path.split("."):
        if not isinstance(current, dict) or part not in current:
            return None
        current = current[part]
    return current


def require(errors: list[str], condition: bool, label: str) -> None:
    if not condition:
        errors.append(label)


def validate_case_map(
    data: dict[str, Any],
    parent_key: str,
    cases: dict[str, str],
    errors: list[str],
) -> None:
    matrix = get_path(data, f"{parent_key}.matrix")
    if not isinstance(matrix, dict):
        errors.append(f"{parent_key}.matrix must be an object with signed Testnet cases")
        return
    for key, label in cases.items():
        item = matrix.get(key)
        if not isinstance(item, dict):
            errors.append(f"{label} evidence is missing")
            continue
        require(errors, item.get("passed") is True, f"{label} must be marked passed=true")
        require(
            errors,
            ISO_UTC.fullmatch(str(item.get("run_at_utc", ""))) is not None,
            f"{label} must include run_at_utc",
        )
        require(
            errors,
            is_non_empty_string(item.get("evidence")),
            f"{label} must include a non-secret evidence note",
        )


def validate_unified_routing(data: dict[str, Any], errors: list[str]) -> None:
    unified = data.get("unified_routing")
    if not isinstance(unified, dict):
        errors.append("unified_routing evidence object is missing")
        return
    require(
        errors,
        ISO_UTC.fullmatch(str(unified.get("run_at_utc", ""))) is not None,
        "unified_routing.run_at_utc must be present",
    )
    validate_case_map(data, "unified_routing", UNIFIED_ROUTING_CASES, errors)

    combo = unified.get("orderbook_native_external_combo")
    if not isinstance(combo, dict):
        errors.append("Unified routing orderbook/native/external combo evidence is missing")
        return
    require(errors, combo.get("passed") is True, "Unified routing combo must be passed=true")
    sources = combo.get("sources_seen")
    require(
        errors,
        isinstance(sources, list)
        and {"orderbook", "native_pool"}.issubset(set(sources))
        and bool({"external_pool", "external_router", "tinyman", "pact"} & set(sources)),
        "Unified routing combo must show orderbook, native_pool, and an external source",
    )
    confirmed_tx_ids = combo.get("confirmed_tx_ids")
    require(
        errors,
        isinstance(confirmed_tx_ids, list)
        and bool(confirmed_tx_ids)
        and all(is_tx_id(item) for item in confirmed_tx_ids),
        "Unified routing combo must include confirmed Testnet transaction IDs",
    )
    require(
        errors,
        is_positive_int(combo.get("source_round")),
        "Unified routing combo must include a source_round",
    )


def validate_dex_additional(data: dict[str, Any], errors: list[str]) -> None:
    dex = data.get("dex_additional_matrix")
    if not isinstance(dex, dict):
        errors.append("dex_additional_matrix evidence object is missing")
        return
    require(
        errors,
        ISO_UTC.fullmatch(str(dex.get("run_at_utc", ""))) is not None,
        "dex_additional_matrix.run_at_utc must be present",
    )
    validate_case_map(data, "dex_additional_matrix", DEX_ADDITIONAL_CASES, errors)

    tx_ids = dex.get("confirmed_tx_ids")
    require(
        errors,
        isinstance(tx_ids, list) and bool(tx_ids) and all(is_tx_id(item) for item in tx_ids),
        "dex_additional_matrix must include confirmed Testnet transaction IDs",
    )
    wallets = dex.get("wallet_addresses")
    require(
        errors,
        isinstance(wallets, list) and len([item for item in wallets if is_non_empty_string(item)]) >= 2,
        "dex_additional_matrix must include at least two public wallet addresses",
    )


def validate_evidence(data: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(data, dict):
        return ["Testnet evidence root must be an object"]
    require(errors, data.get("network") == "testnet", "Testnet evidence network must be testnet")
    validate_unified_routing(data, errors)
    validate_dex_additional(data, errors)
    return errors


def case_status(item: Any) -> str:
    if not isinstance(item, dict):
        return "missing"
    if item.get("passed") is not True:
        return "incomplete"
    if ISO_UTC.fullmatch(str(item.get("run_at_utc", ""))) is None:
        return "incomplete"
    if not is_non_empty_string(item.get("evidence")):
        return "incomplete"
    return "passed"


def section_matrix_report(data: dict[str, Any], section_key: str) -> list[dict[str, Any]]:
    section = data.get(section_key)
    matrix = section.get("matrix") if isinstance(section, dict) else None
    cases = SECTION_CASES[section_key]
    out = []
    for key, label in cases.items():
        item = matrix.get(key) if isinstance(matrix, dict) else None
        out.append(
            {
                "section": section_key,
                "case": key,
                "label": label,
                "status": case_status(item),
            }
        )
    return out


def unified_combo_status(combo: Any) -> str:
    if not isinstance(combo, dict):
        return "missing"
    sources = combo.get("sources_seen")
    confirmed_tx_ids = combo.get("confirmed_tx_ids")
    if (
        combo.get("passed") is True
        and isinstance(sources, list)
        and {"orderbook", "native_pool"}.issubset(set(sources))
        and bool({"external_pool", "external_router", "tinyman", "pact"} & set(sources))
        and isinstance(confirmed_tx_ids, list)
        and bool(confirmed_tx_ids)
        and all(is_tx_id(item) for item in confirmed_tx_ids)
        and is_positive_int(combo.get("source_round"))
    ):
        return "passed"
    return "incomplete"


def build_report(data: Any) -> dict[str, Any]:
    errors = validate_evidence(data)
    root = data if isinstance(data, dict) else {}
    sections: dict[str, Any] = {}
    for section_key in SECTION_CASES:
        cases = section_matrix_report(root, section_key)
        sections[section_key] = {
            "present": isinstance(root.get(section_key), dict),
            "run_at_utc": root.get(section_key, {}).get("run_at_utc")
            if isinstance(root.get(section_key), dict)
            else None,
            "cases": cases,
            "missing_cases": [item for item in cases if item["status"] != "passed"],
        }

    unified = root.get("unified_routing")
    combo = unified.get("orderbook_native_external_combo") if isinstance(unified, dict) else None
    sections["unified_routing"]["combo_status"] = unified_combo_status(combo)

    return {
        "complete": not errors,
        "error_count": len(errors),
        "errors": errors,
        "sections": sections,
    }


def print_guide(report: dict[str, Any]) -> None:
    print("Signed Testnet evidence guide")
    print()
    print("1. Run the matching flow in the OpenNodia web UI and copy the non-secret QA fragment.")
    print("2. Save the fragment outside the repository, for example /tmp/dex-submit-fragment.json.")
    print("3. Merge only real browser-signed Testnet evidence with one of these commands.")
    print()
    print("Batch merge all copied fragments that include matrix_case_hints:")
    print("  python3 scripts/qa/testnet_plan_evidence.py fragments \\")
    print("    --evidence \"Real browser-signed Testnet validation batch\" \\")
    print("    /tmp/opennodia-qa/*.json")
    print()
    print("Batch merge fragments and explicitly record the unified combo when the set covers it:")
    print("  python3 scripts/qa/testnet_plan_evidence.py fragments \\")
    print("    --combine-unified-combo \\")
    print("    --evidence \"Real browser-signed Testnet validation batch\" \\")
    print("    --combo-evidence \"Real browser-signed Testnet route covering orderbook, native AMM, and external liquidity\" \\")
    print("    /tmp/opennodia-qa/*.json")
    print()
    for section_key, section in report["sections"].items():
        for item in section["missing_cases"]:
            print(f"- {item['section']}.{item['case']}: {item['label']}")
            print("  python3 scripts/qa/testnet_plan_evidence.py case \\")
            print(f"    --section {item['section']} \\")
            print(f"    --case {item['case']} \\")
            print("    --fragment /tmp/dex-submit-fragment.json \\")
            print('    --evidence "Real browser-signed Testnet validation" \\')
            print("    --run-at-utc YYYY-MM-DDTHH:MM:SSZ")
            print()
    if report["sections"]["unified_routing"]["combo_status"] != "passed":
        print("- unified_routing.orderbook_native_external_combo: orderbook/native/external combo")
        print("  python3 scripts/qa/testnet_plan_evidence.py unified-combo \\")
        print("    --fragment /tmp/router-submit-fragment.json \\")
        print('    --evidence "Real browser-signed Testnet route covering orderbook, native AMM, and external liquidity" \\')
        print("    --run-at-utc YYYY-MM-DDTHH:MM:SSZ")
        print()
    print("Do not pass PINs, mnemonics, tokens, session values, signed blobs, or wallet handles to these commands.")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Report missing evidence for plans/unified-routing.md and plans/dex-backlog.md."
    )
    parser.add_argument(
        "evidence",
        nargs="?",
        type=Path,
        default=DEFAULT_EVIDENCE,
        help="Path to scripts/qa/testnet-evidence.json",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit non-zero when any required plan evidence is missing.",
    )
    output = parser.add_mutually_exclusive_group()
    output.add_argument(
        "--json",
        action="store_true",
        help="Print a machine-readable report of missing plan evidence.",
    )
    output.add_argument(
        "--guide",
        action="store_true",
        help="Print operator merge commands for missing signed Testnet evidence.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    strict = args.strict or os.environ.get("OPENNODIA_PLAN_READINESS_STRICT") == "true"
    data = read_json(args.evidence)
    report = build_report(data)
    errors = report["errors"]
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
        return 1 if errors and strict else 0
    if args.guide:
        print_guide(report)
        return 1 if errors and strict else 0
    if not errors:
        print("Testnet plan evidence is complete.")
        return 0
    for error in errors:
        print(f"MISSING: {error}", file=sys.stderr)
    print(f"Testnet plan evidence is incomplete: {len(errors)} missing item(s).")
    return 1 if strict else 0


if __name__ == "__main__":
    raise SystemExit(main())
