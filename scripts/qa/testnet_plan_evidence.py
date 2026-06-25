#!/usr/bin/env python3
"""Merge non-secret signed Testnet QA fragments into plan evidence."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_EVIDENCE = REPO_ROOT / "scripts" / "qa" / "testnet-evidence.json"
ISO_UTC = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
TX_ID = re.compile(r"^[A-Z2-7]{52}$")
ANSI_ESCAPE = re.compile(r"\x1b\[[0-9;]*m")
SECRET_KEY_RE = re.compile(
    r"(pin|mnemonic|token|secret|password|session|wallet_handle|signed_tx|signed_blob)",
    re.IGNORECASE,
)
SECRET_VALUE_RE = re.compile(
    r"(mnemonic|wallet_handle_token|Authorization:|Bearer\s+[A-Za-z0-9._-]+)",
    re.IGNORECASE,
)

UNIFIED_ROUTING_CASES = {
    "orderbook_only_pair",
    "native_pool_only_pair",
    "tinyman_pact_pool_only_pair",
    "orderbook_amm_price_cross_pair",
    "network_fee_changes_best_route",
    "limit_partial_then_standing_remainder",
    "quote_then_state_change_rejected",
    "pair_reversal_decimals",
    "duplicate_folks_backed_pool",
}
DEX_ADDITIONAL_CASES = {
    "two_wallet_create_fill_cancel",
    "algo_asa_bid_ask_reverse",
    "asa_algo_bid_ask_reverse",
    "asa_asa_bid_ask_reverse",
    "ioc_no_match_balance",
    "ioc_single_fill_balance",
    "ioc_multi_fill_balance",
    "ioc_discarded_remainder_balance",
    "expiry_boundary",
    "duplicate_submit",
    "intent_reuse",
    "server_restart",
    "stale_node_fallback",
    "artifact_secret_scan",
}
SECTION_CASES = {
    "unified_routing": UNIFIED_ROUTING_CASES,
    "dex_additional_matrix": DEX_ADDITIONAL_CASES,
}
SECTIONS = set(SECTION_CASES)
DEX_ORDERBOOK_VIEW_CASES = {
    "algo_asa_bid_ask_reverse",
    "asa_algo_bid_ask_reverse",
    "asa_asa_bid_ask_reverse",
}


def read_json(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise SystemExit(f"{label} is missing: {path}") from None
    except json.JSONDecodeError as error:
        raise SystemExit(f"{label} is not valid JSON: {path}: {error}") from None


def write_json(path: Path, data: Any) -> None:
    path.write_text(json.dumps(data, indent=2, sort_keys=False) + "\n", encoding="utf-8")


def utc_now() -> str:
    return dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def run_command(cmd: list[str], cwd: Path) -> str:
    result = subprocess.run(cmd, cwd=cwd, check=True, text=True, capture_output=True)
    return (result.stdout or result.stderr).strip()


def get_json_url(url: str, attempts: int = 1, delay_secs: float = 1.0) -> Any:
    last_error: Exception | None = None
    for attempt in range(attempts):
        try:
            with urllib.request.urlopen(url, timeout=10) as response:
                return json.loads(response.read().decode("utf-8"))
        except (urllib.error.URLError, json.JSONDecodeError) as error:
            last_error = error
            if attempt + 1 < attempts:
                time.sleep(delay_secs)
    if isinstance(last_error, json.JSONDecodeError):
        raise SystemExit(f"GET {url} did not return JSON: {last_error}") from last_error
    raise SystemExit(f"GET {url} failed: {last_error}") from last_error


def validate_api_status(value: Any, label: str) -> None:
    if not isinstance(value, dict):
        raise SystemExit(f"{label} must be a JSON object")
    if value.get("network") != "testnet":
        raise SystemExit(f"{label} must report network=testnet")
    if value.get("setup_complete") is not True:
        raise SystemExit(f"{label} must report setup_complete=true")
    if value.get("node_reachable") is not True:
        raise SystemExit(f"{label} must report node_reachable=true")


def strip_ansi(value: str) -> str:
    return ANSI_ESCAPE.sub("", value)


def parse_dex_public_reconciliation(logs: str) -> dict[str, Any] | None:
    latest: dict[str, Any] | None = None
    scanned = 0
    for line in logs.splitlines():
        scanned += 1
        clean = strip_ansi(line)
        if "DEX reconciliation sweep completed" not in clean or "source=Public" not in clean:
            continue

        def number_field(name: str) -> int | None:
            match = re.search(rf"\b{name}=([0-9]+)", clean)
            return int(match.group(1)) if match else None

        latest = {
            "source": "public",
            "round": number_field("round"),
            "checked": number_field("checked"),
            "changes": number_field("changes"),
            "errors": number_field("errors"),
            "line_number": scanned,
        }
    if latest:
        latest["lines_scanned"] = scanned
    return latest


def reject_secret_material(value: Any, path: str = "$") -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            if SECRET_KEY_RE.search(str(key)):
                raise SystemExit(f"refusing to merge secret-like key at {path}.{key}")
            reject_secret_material(child, f"{path}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            reject_secret_material(child, f"{path}[{index}]")
    elif isinstance(value, str) and SECRET_VALUE_RE.search(value):
        raise SystemExit(f"refusing to merge secret-like value at {path}")


def ensure_evidence_root(path: Path) -> dict[str, Any]:
    data = read_json(path, "Testnet evidence")
    if not isinstance(data, dict):
        raise SystemExit("Testnet evidence root must be an object")
    if data.get("network") != "testnet":
        raise SystemExit("Testnet evidence network must be testnet")
    return data


def require_iso(value: str, label: str) -> str:
    if not ISO_UTC.fullmatch(value):
        raise SystemExit(f"{label} must be UTC ISO timestamp like 2026-06-20T00:00:00Z")
    return value


def fragment_run_at(fragment: Any, fallback: str | None = None) -> str:
    if fallback:
        return require_iso(fallback, "run_at_utc")
    if isinstance(fragment, dict):
        run_at = fragment.get("run_at_utc")
        if isinstance(run_at, str):
            return require_iso(run_at, "fragment.run_at_utc")
    raise SystemExit("run_at_utc is required when the fragment does not include run_at_utc")


def tx_ids_from_fragment(fragment: Any) -> list[str]:
    out: list[str] = []

    def visit(value: Any) -> None:
        if isinstance(value, dict):
            for key, child in value.items():
                if (
                    (key in {"tx_id", "txid"} or key.endswith("_tx_id"))
                    and isinstance(child, str)
                    and TX_ID.fullmatch(child)
                ):
                    out.append(child)
                elif (key == "tx_ids" or key.endswith("_tx_ids")) and isinstance(child, list):
                    out.extend(item for item in child if isinstance(item, str) and TX_ID.fullmatch(item))
                else:
                    visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)

    visit(fragment)
    return sorted(set(out))


def public_addresses_from_fragment(fragment: Any) -> list[str]:
    out: list[str] = []
    public_wallet_keys = {
        "wallet_address",
        "owner_address",
        "filler_address",
        "trader_address",
        "sender_address",
        "operator_address",
    }

    def visit(value: Any) -> None:
        if isinstance(value, dict):
            for key, child in value.items():
                if key in public_wallet_keys and isinstance(child, str) and len(child) >= 32:
                    out.append(child)
                else:
                    visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)

    visit(fragment)
    return sorted(set(out))


def source_round_from_fragment(fragment: Any) -> int | None:
    candidates: list[int] = []

    def visit(value: Any) -> None:
        if isinstance(value, dict):
            for key, child in value.items():
                if key in {"source_round", "confirmed_round"} and isinstance(child, int) and child > 0:
                    candidates.append(child)
                else:
                    visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)

    visit(fragment)
    return max(candidates) if candidates else None


def positive_int(value: Any) -> bool:
    return isinstance(value, int) and value > 0


def view_has_bid_ask_depth(view: Any) -> bool:
    if not isinstance(view, dict):
        return False
    bids = view.get("bids")
    asks = view.get("asks")
    return isinstance(bids, list) and bool(bids) and isinstance(asks, list) and bool(asks)


def validate_orderbook_view_case(case_key: str, fragment: Any) -> None:
    if not isinstance(fragment, dict):
        raise SystemExit(f"dex_additional_matrix.{case_key} requires an orderbook view fragment object")
    if fragment.get("kind") != "orderbook_view":
        raise SystemExit(f"dex_additional_matrix.{case_key} requires kind=orderbook_view")
    base_asset_id = fragment.get("base_asset_id")
    quote_asset_id = fragment.get("quote_asset_id")
    if case_key == "algo_asa_bid_ask_reverse" and not (base_asset_id == 0 and positive_int(quote_asset_id)):
        raise SystemExit("algo_asa_bid_ask_reverse requires base_asset_id=0 and a non-zero quote_asset_id")
    if case_key == "asa_algo_bid_ask_reverse" and not (positive_int(base_asset_id) and quote_asset_id == 0):
        raise SystemExit("asa_algo_bid_ask_reverse requires a non-zero base_asset_id and quote_asset_id=0")
    if case_key == "asa_asa_bid_ask_reverse" and not (positive_int(base_asset_id) and positive_int(quote_asset_id)):
        raise SystemExit("asa_asa_bid_ask_reverse requires non-zero base_asset_id and quote_asset_id")

    current_view = fragment.get("current_view")
    reverse_view = fragment.get("reverse_view")
    if not view_has_bid_ask_depth(current_view):
        raise SystemExit(f"dex_additional_matrix.{case_key} requires current_view bids and asks")
    if not view_has_bid_ask_depth(reverse_view):
        raise SystemExit(f"dex_additional_matrix.{case_key} requires reverse_view bids and asks")
    if isinstance(current_view, dict) and current_view.get("view_asset_id") != base_asset_id:
        raise SystemExit(f"dex_additional_matrix.{case_key} current_view.view_asset_id must match base_asset_id")
    if isinstance(reverse_view, dict) and reverse_view.get("view_asset_id") != quote_asset_id:
        raise SystemExit(f"dex_additional_matrix.{case_key} reverse_view.view_asset_id must match quote_asset_id")


def validate_matrix_case_fragment(section_key: str, case_key: str, fragment: Any) -> None:
    if section_key == "dex_additional_matrix" and case_key in DEX_ORDERBOOK_VIEW_CASES:
        validate_orderbook_view_case(case_key, fragment)


def sources_from_fragment(fragment: Any) -> list[str]:
    out: set[str] = set()

    def add(value: Any) -> None:
        if not isinstance(value, str):
            return
        source = value.strip().lower()
        if not source:
            return
        if source in {"orderbook", "native_orderbook"} or "orderbook" in source:
            out.add("orderbook")
        if source in {"native_pool", "native_amm"} or "native pool" in source:
            out.add("native_pool")
        if source in {"external_pool", "external_router"}:
            out.add(source)
        if "tinyman" in source:
            out.update({"external_pool", "tinyman"})
        if "pact" in source:
            out.update({"external_pool", "pact"})

    def visit(value: Any) -> None:
        if isinstance(value, dict):
            for key, child in value.items():
                if key == "sources_seen" and isinstance(child, list):
                    for item in child:
                        add(item)
                elif key in {"source", "source_id", "source_label", "source_type"}:
                    add(child)
                    visit(child)
                else:
                    visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)

    visit(fragment)
    return sorted(out)


def merge_matrix_case(
    data: dict[str, Any],
    section_key: str,
    case_key: str,
    run_at: str,
    evidence: str,
    fragment: Any,
) -> None:
    if section_key not in SECTION_CASES:
        raise SystemExit(f"section must be one of: {', '.join(sorted(SECTIONS))}")
    if case_key not in SECTION_CASES[section_key]:
        raise SystemExit(f"{section_key}.{case_key} is not a known readiness matrix case")
    validate_matrix_case_fragment(section_key, case_key, fragment)

    section = data.setdefault(section_key, {})
    if not isinstance(section, dict):
        raise SystemExit(f"{section_key} must be an object")
    section.setdefault("run_at_utc", run_at)
    matrix = section.setdefault("matrix", {})
    if not isinstance(matrix, dict):
        raise SystemExit(f"{section_key}.matrix must be an object")
    matrix[case_key] = {
        "passed": True,
        "run_at_utc": run_at,
        "evidence": evidence,
        "fragment": fragment,
    }

    tx_ids = tx_ids_from_fragment(fragment)
    if section_key == "dex_additional_matrix":
        existing_tx_ids = section.setdefault("confirmed_tx_ids", [])
        if not isinstance(existing_tx_ids, list):
            raise SystemExit("dex_additional_matrix.confirmed_tx_ids must be a list")
        section["confirmed_tx_ids"] = sorted(set(existing_tx_ids + tx_ids))
        existing_wallets = section.setdefault("wallet_addresses", [])
        if not isinstance(existing_wallets, list):
            raise SystemExit("dex_additional_matrix.wallet_addresses must be a list")
        section["wallet_addresses"] = sorted(set(existing_wallets + public_addresses_from_fragment(fragment)))


def matrix_hints_from_fragment(fragment: Any) -> dict[str, list[str]]:
    hints = fragment.get("matrix_case_hints") if isinstance(fragment, dict) else None
    if not isinstance(hints, dict):
        raise SystemExit("auto-cases fragment must include matrix_case_hints")

    out: dict[str, list[str]] = {}
    for section_key in sorted(SECTIONS):
        raw = hints.get(section_key, [])
        if raw is None:
            raw = []
        if not isinstance(raw, list):
            raise SystemExit(f"matrix_case_hints.{section_key} must be a list")
        cases = []
        for case_key in raw:
            if not isinstance(case_key, str):
                raise SystemExit(f"matrix_case_hints.{section_key} entries must be strings")
            if case_key not in SECTION_CASES[section_key]:
                raise SystemExit(f"{section_key}.{case_key} is not a known readiness matrix case")
            cases.append(case_key)
        out[section_key] = sorted(set(cases))
    if not any(out.values()):
        raise SystemExit("auto-cases fragment did not include any known matrix case hints")
    return out


def merge_case(args: argparse.Namespace) -> int:
    run_at = require_iso(args.run_at_utc, "run_at_utc")
    data = ensure_evidence_root(args.output)
    fragment = read_json(args.fragment, "QA fragment")
    reject_secret_material(fragment)
    merge_matrix_case(data, args.section, args.case, run_at, args.evidence, fragment)
    write_json(args.output, data)
    print(f"Merged {args.section}.matrix.{args.case} into {args.output}")
    return 0


def merge_auto_cases(args: argparse.Namespace) -> int:
    run_at = require_iso(args.run_at_utc, "run_at_utc")
    data = ensure_evidence_root(args.output)
    fragment = read_json(args.fragment, "QA fragment")
    reject_secret_material(fragment)
    hints = matrix_hints_from_fragment(fragment)
    merged = []
    for section_key, cases in hints.items():
        for case_key in cases:
            merge_matrix_case(data, section_key, case_key, run_at, args.evidence, fragment)
            merged.append(f"{section_key}.matrix.{case_key}")
    write_json(args.output, data)
    print(f"Merged {', '.join(merged)} into {args.output}")
    return 0


def merge_unified_combo_fragment(
    data: dict[str, Any],
    fragment: Any,
    run_at: str,
    evidence: str,
    source_overrides: list[str] | None = None,
) -> None:
    tx_ids = tx_ids_from_fragment(fragment)
    source_round = source_round_from_fragment(fragment)
    if not tx_ids:
        raise SystemExit("combo fragment must include at least one confirmed Testnet tx id")
    if source_round is None:
        raise SystemExit("combo fragment must include source_round or confirmed_round")
    sources = sources_from_fragment(fragment)
    if source_overrides:
        sources = sorted(set(sources + source_overrides))
    if not sources:
        raise SystemExit("combo fragment must include sources_seen/source_type values or --sources")

    unified = data.setdefault("unified_routing", {})
    if not isinstance(unified, dict):
        raise SystemExit("unified_routing must be an object")
    unified.setdefault("run_at_utc", run_at)
    unified["orderbook_native_external_combo"] = {
        "passed": True,
        "run_at_utc": run_at,
        "evidence": evidence,
        "sources_seen": sources,
        "confirmed_tx_ids": tx_ids,
        "source_round": source_round,
        "fragment": fragment,
    }


def merge_unified_combo(args: argparse.Namespace) -> int:
    run_at = require_iso(args.run_at_utc, "run_at_utc")
    data = ensure_evidence_root(args.output)
    fragment = read_json(args.fragment, "QA fragment")
    reject_secret_material(fragment)
    source_overrides = [item.strip() for item in args.sources.split(",") if item.strip()] if args.sources else None
    merge_unified_combo_fragment(data, fragment, run_at, args.evidence, source_overrides)
    write_json(args.output, data)
    print(f"Merged unified_routing.orderbook_native_external_combo into {args.output}")
    return 0


def merge_fragments(args: argparse.Namespace) -> int:
    data = ensure_evidence_root(args.output)
    merged = []
    combo_fragments = []
    combo_run_at = args.run_at_utc
    for fragment_path in args.fragments:
        fragment = read_json(fragment_path, f"QA fragment {fragment_path}")
        reject_secret_material(fragment)
        run_at = fragment_run_at(fragment, args.run_at_utc)
        hints = matrix_hints_from_fragment(fragment)
        for section_key, cases in hints.items():
            for case_key in cases:
                merge_matrix_case(data, section_key, case_key, run_at, args.evidence, fragment)
                merged.append(f"{section_key}.matrix.{case_key}")
        if args.combine_unified_combo:
            combo_fragments.append(fragment)
            if combo_run_at is None:
                combo_run_at = run_at
            else:
                combo_run_at = max(combo_run_at, run_at)

    if args.combine_unified_combo:
        if not combo_fragments:
            raise SystemExit("no fragments were available for unified combo merge")
        combined_fragment = {
            "network": "testnet",
            "run_at_utc": combo_run_at,
            "sources_seen": sorted({source for fragment in combo_fragments for source in sources_from_fragment(fragment)}),
            "confirmed_tx_ids": sorted({tx_id for fragment in combo_fragments for tx_id in tx_ids_from_fragment(fragment)}),
            "source_round": max(
                (round_value for fragment in combo_fragments if (round_value := source_round_from_fragment(fragment)) is not None),
                default=None,
            ),
            "fragments": combo_fragments,
        }
        source_overrides = (
            [item.strip() for item in args.sources.split(",") if item.strip()] if args.sources else None
        )
        merge_unified_combo_fragment(
            data,
            combined_fragment,
            require_iso(str(combo_run_at), "run_at_utc"),
            args.combo_evidence or args.evidence,
            source_overrides,
        )
        merged.append("unified_routing.orderbook_native_external_combo")

    write_json(args.output, data)
    print(f"Merged {', '.join(sorted(set(merged)))} into {args.output}")
    return 0


def merge_server_restart(args: argparse.Namespace) -> int:
    if not args.confirm_restart:
        raise SystemExit("pass --confirm-restart only when restarting the local Testnet OpenNodia service is intended")
    run_at = require_iso(args.run_at_utc or utc_now(), "run_at_utc")
    data = ensure_evidence_root(args.output)
    status_before = get_json_url(args.api_url, attempts=10)
    validate_api_status(status_before, "status before restart")
    restart_output = run_command(["docker", "compose", "restart", args.compose_service], args.compose_dir)
    status_after = get_json_url(args.api_url, attempts=30)
    validate_api_status(status_after, "status after restart")

    fragment = {
        "network": "testnet",
        "run_at_utc": run_at,
        "api_url": args.api_url,
        "compose_service": args.compose_service,
        "status_before": status_before,
        "status_after": status_after,
        "restart_output_tail": restart_output[-500:] if restart_output else "",
        "matrix_case_hints": {
            "dex_additional_matrix": ["server_restart"],
            "unified_routing": [],
        },
    }
    reject_secret_material(fragment)
    merge_matrix_case(
        data,
        "dex_additional_matrix",
        "server_restart",
        run_at,
        args.evidence,
        fragment,
    )
    write_json(args.output, data)
    print(f"Merged dex_additional_matrix.matrix.server_restart into {args.output}")
    return 0


def merge_stale_node_fallback(args: argparse.Namespace) -> int:
    run_at = require_iso(args.run_at_utc or utc_now(), "run_at_utc")
    data = ensure_evidence_root(args.output)
    status = get_json_url(args.api_url, attempts=10)
    validate_api_status(status, "status during stale-node fallback check")
    logs = run_command(
        ["docker", "compose", "logs", "--tail", str(args.log_tail), args.compose_service],
        args.compose_dir,
    )
    reconciliation = parse_dex_public_reconciliation(logs)
    if not reconciliation:
        raise SystemExit(
            "no DEX reconciliation sweep with source=Public was found in the selected service logs"
        )
    if reconciliation.get("errors") not in {0, None}:
        raise SystemExit("DEX public fallback reconciliation reported errors")

    fragment = {
        "network": "testnet",
        "run_at_utc": run_at,
        "api_url": args.api_url,
        "compose_service": args.compose_service,
        "status": status,
        "dex_reconciliation": reconciliation,
        "matrix_case_hints": {
            "dex_additional_matrix": ["stale_node_fallback"],
            "unified_routing": [],
        },
    }
    reject_secret_material(fragment)
    merge_matrix_case(
        data,
        "dex_additional_matrix",
        "stale_node_fallback",
        run_at,
        args.evidence,
        fragment,
    )
    write_json(args.output, data)
    print(f"Merged dex_additional_matrix.matrix.stale_node_fallback into {args.output}")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Merge real browser-copied signed Testnet QA fragments into scripts/qa/testnet-evidence.json."
    )
    sub = parser.add_subparsers(dest="command", required=True)

    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--fragment", type=Path, required=True, help="Browser-copied non-secret QA JSON")
    common.add_argument("--evidence", required=True, help="Short non-secret note about the real validation")
    common.add_argument("--output", type=Path, default=DEFAULT_EVIDENCE, help="Evidence file to update")
    common.add_argument("--run-at-utc", required=True, help="Validation timestamp in UTC")

    case = sub.add_parser("case", parents=[common], help="Mark one matrix case from a real fragment")
    case.add_argument("--section", required=True, choices=sorted(SECTIONS))
    case.add_argument("--case", required=True, help="Matrix case key from testnet_plan_readiness.py")
    case.set_defaults(func=merge_case)

    auto = sub.add_parser(
        "auto-cases",
        parents=[common],
        description="Merge all matrix_case_hints from one browser-copied fragment.",
        help="Merge all matrix_case_hints from one browser-copied fragment",
    )
    auto.set_defaults(func=merge_auto_cases)

    combo = sub.add_parser(
        "unified-combo",
        parents=[common],
        help="Record the unified orderbook/native/external combo evidence",
    )
    combo.add_argument(
        "--sources",
        help="Optional comma-separated sources seen, e.g. orderbook,native_pool,tinyman",
    )
    combo.set_defaults(func=merge_unified_combo)

    fragments = sub.add_parser(
        "fragments",
        help="Merge matrix hints from multiple browser-copied fragments",
        description=(
            "Merge matrix_case_hints from multiple non-secret browser-copied fragments. "
            "Use --combine-unified-combo only after the fragment set represents one real "
            "signed Testnet routing validation batch covering orderbook, native AMM, and external liquidity."
        ),
    )
    fragments.add_argument("fragments", type=Path, nargs="+", help="Browser-copied non-secret QA JSON files")
    fragments.add_argument("--evidence", required=True, help="Short non-secret note about the real validation batch")
    fragments.add_argument("--output", type=Path, default=DEFAULT_EVIDENCE, help="Evidence file to update")
    fragments.add_argument(
        "--run-at-utc",
        help="Validation timestamp in UTC. Defaults to each fragment run_at_utc when present.",
    )
    fragments.add_argument(
        "--combine-unified-combo",
        action="store_true",
        help="Also record unified_routing.orderbook_native_external_combo from the fragment set",
    )
    fragments.add_argument(
        "--combo-evidence",
        help="Optional separate non-secret evidence note for the combined unified routing combo",
    )
    fragments.add_argument(
        "--sources",
        help="Optional comma-separated combo sources seen, e.g. orderbook,native_pool,tinyman",
    )
    fragments.set_defaults(func=merge_fragments)

    restart = sub.add_parser(
        "server-restart",
        help="Restart the local service and record real Testnet DEX server-restart evidence",
    )
    restart.add_argument("--api-url", default="http://127.0.0.1:30080/api/status")
    restart.add_argument("--compose-dir", type=Path, default=REPO_ROOT)
    restart.add_argument("--compose-service", default="opennodia")
    restart.add_argument("--confirm-restart", action="store_true")
    restart.add_argument("--output", type=Path, default=DEFAULT_EVIDENCE, help="Evidence file to update")
    restart.add_argument(
        "--run-at-utc",
        help="Validation timestamp in UTC. Defaults to the current UTC time.",
    )
    restart.add_argument(
        "--evidence",
        default="OpenNodia Testnet API remained ready after docker compose service restart",
        help="Short non-secret note about the real validation",
    )
    restart.set_defaults(func=merge_server_restart)

    stale = sub.add_parser(
        "stale-node-fallback",
        help="Record real Testnet DEX public-source fallback evidence from service logs",
    )
    stale.add_argument("--api-url", default="http://127.0.0.1:30080/api/status")
    stale.add_argument("--compose-dir", type=Path, default=REPO_ROOT)
    stale.add_argument("--compose-service", default="opennodia")
    stale.add_argument("--log-tail", type=int, default=300)
    stale.add_argument("--output", type=Path, default=DEFAULT_EVIDENCE, help="Evidence file to update")
    stale.add_argument(
        "--run-at-utc",
        help="Validation timestamp in UTC. Defaults to the current UTC time.",
    )
    stale.add_argument(
        "--evidence",
        default="OpenNodia DEX reconciliation used the configured public Testnet source while keeping API status healthy",
        help="Short non-secret note about the real validation",
    )
    stale.set_defaults(func=merge_stale_node_fallback)

    return parser.parse_args()


def main() -> int:
    args = parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
