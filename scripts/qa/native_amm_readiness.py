#!/usr/bin/env python3
"""Report native AMM validation coverage without using signing secrets."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]

REQUIRED_SNIPPETS = {
    "contract_guard_matrix": {
        "path": "crates/opennodia-amm/src/contract.rs",
        "snippets": [
            "approval_source_enforces_universal_call_guards",
            "bootstrap_and_add_guard_matrix_is_present",
            "remove_guard_matrix_is_present",
            "swap_v3_guard_matrix_is_present",
            "deposit_subroutines_reject_close_rekey_and_fee_mutations",
            "txn Fee\nint {max_txn_fee}\n<=",
            "RekeyTo\nglobal ZeroAddress",
            "CloseRemainderTo\nglobal ZeroAddress",
            "AssetCloseTo\nglobal ZeroAddress",
            "global Round\ntxna ApplicationArgs",
        ],
    },
    "math_stateful_matrix": {
        "path": "crates/opennodia-amm/src/lib.rs",
        "snippets": [
            "cpmm_golden_vectors_cover_fee_tiers_and_directions",
            "stateful_quote_sequence_preserves_pool_invariants",
            "donation_imbalanced_stateful_sequence_preserves_lp_accounting",
            "initial_liquidity_rounding_never_mints_locked_lp",
            "imbalanced_deposits_do_not_mint_against_donated_side",
            "add_liquidity_rounding_cannot_inflate_lp_supply",
            "tiny_swap_inputs_do_not_create_zero_output_quotes",
            "stateful_rounding_edge_sequence_preserves_locked_lp",
            "remove_liquidity_rounding_cannot_touch_locked_liquidity",
            "swaps_preserve_constant_product_after_fee_rounding",
        ],
    },
    "mainnet_fail_closed_gate": {
        "paths": [
            "crates/opennodia-server/src/lp.rs",
            "crates/opennodia-server/src/lp/guards.rs",
        ],
        "snippets": [
            "native_amm_writes_allowed_for",
            "network != Network::Mainnet || mainnet_write_enabled_after_audit",
            "ensure_native_amm_writes_allowed",
            "mainnet native AMM writes are fail-closed until independent audit opt-in is enabled",
            "native_amm_writes_stay_fail_closed_on_mainnet_without_audit_opt_in",
        ],
    },
}


def read_text(relative_path: str) -> str:
    path = REPO_ROOT / relative_path
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        raise SystemExit(f"required file is missing: {path}") from None


def static_checks() -> list[dict[str, Any]]:
    checks: list[dict[str, Any]] = []
    for name, item in REQUIRED_SNIPPETS.items():
        paths = item["paths"] if "paths" in item else [item["path"]]
        source = "\n".join(read_text(path) for path in paths)
        missing = [snippet for snippet in item["snippets"] if snippet not in source]
        checks.append(
            {
                "name": name,
                "path": ", ".join(paths),
                "passed": not missing,
                "missing": missing,
            }
        )
    return checks


def run_command(command: list[str]) -> dict[str, Any]:
    result = subprocess.run(
        command,
        cwd=REPO_ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    return {
        "command": command,
        "passed": result.returncode == 0,
        "returncode": result.returncode,
        "output_tail": result.stdout[-4000:],
    }


def api_status(api_url: str) -> dict[str, Any]:
    url = api_url.rstrip("/") + "/api/status"
    try:
        with urllib.request.urlopen(url, timeout=10) as response:
            data = json.loads(response.read().decode("utf-8"))
    except (OSError, urllib.error.URLError, json.JSONDecodeError) as error:
        return {"url": url, "passed": False, "error": str(error)}
    return {
        "url": url,
        "passed": data.get("setup_complete") is True
        and data.get("node_reachable") is True
        and data.get("network") == "testnet",
        "status": data,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Check native AMM guard, math, and mainnet fail-closed readiness."
    )
    parser.add_argument(
        "--run-cargo",
        action="store_true",
        help="Run targeted cargo tests that cover the native AMM readiness matrix.",
    )
    parser.add_argument(
        "--api-url",
        help="Optional OpenNodia API URL for public status validation, e.g. http://127.0.0.1:30080.",
    )
    parser.add_argument("--json", action="store_true", help="Print machine-readable JSON.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    checks = static_checks()
    commands = []
    status = None

    if args.run_cargo:
        commands.extend(
            [
                run_command(["cargo", "test", "-p", "opennodia-amm", "--lib"]),
                run_command(
                    [
                        "cargo",
                        "test",
                        "-p",
                        "opennodia-server",
                        "lp::tests::native_amm_writes_stay_fail_closed_on_mainnet_without_audit_opt_in",
                    ]
                ),
            ]
        )

    if args.api_url:
        status = api_status(args.api_url)

    passed = all(item["passed"] for item in checks)
    passed = passed and all(item["passed"] for item in commands)
    if status is not None:
        passed = passed and status["passed"]

    report = {
        "passed": passed,
        "checks": checks,
        "commands": commands,
        "api_status": status,
    }

    if args.json:
        print(json.dumps(report, indent=2))
    else:
        for item in checks:
            state = "ok" if item["passed"] else "missing"
            print(f"{state}: {item['name']} ({item['path']})")
            for snippet in item["missing"]:
                print(f"  missing snippet: {snippet!r}")
        for command in commands:
            state = "ok" if command["passed"] else "failed"
            print(f"{state}: {' '.join(command['command'])}")
            if not command["passed"]:
                print(command["output_tail"], file=sys.stderr)
        if status is not None:
            state = "ok" if status["passed"] else "failed"
            print(f"{state}: {status['url']}")

    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
