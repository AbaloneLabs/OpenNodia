#!/usr/bin/env python3
"""Read Pact factory boxes from a real algod node and report pool candidates."""

from __future__ import annotations

import argparse
import base64
from concurrent.futures import ThreadPoolExecutor, as_completed
import json
import sys
import urllib.error
import urllib.parse
import urllib.request


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Report Pact constant-product factory pool boxes from algod."
    )
    parser.add_argument(
        "--algod-url",
        default="https://testnet-api.algonode.cloud",
        help="Algod base URL. Defaults to Algonode Testnet.",
    )
    parser.add_argument(
        "--algod-token-file",
        help="Optional file containing the algod token. The token is never printed.",
    )
    parser.add_argument(
        "--factory-app-id",
        type=int,
        default=166540424,
        help="Pact factory app ID.",
    )
    parser.add_argument("--asset-a", type=int, help="Optional first asset ID filter.")
    parser.add_argument("--asset-b", type=int, help="Optional second asset ID filter.")
    parser.add_argument(
        "--limit",
        type=int,
        default=1000,
        help="Maximum boxes to request from algod.",
    )
    parser.add_argument(
        "--include-state",
        action="store_true",
        help="Fetch each pool app global state and include reserve/tradable fields.",
    )
    parser.add_argument(
        "--workers",
        type=int,
        default=8,
        help="Concurrent algod requests for box value and pool state reads.",
    )
    return parser.parse_args()


def read_token(path: str | None) -> str:
    if not path:
        return ""
    with open(path, "r", encoding="utf-8") as handle:
        return handle.read().strip()


def request_json(base_url: str, path: str, token: str) -> dict:
    url = base_url.rstrip("/") + path
    request = urllib.request.Request(url)
    if token:
        request.add_header("X-Algo-API-Token", token)
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        body = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"GET {path}: {error.code} {body}") from error
    except urllib.error.URLError as error:
        raise RuntimeError(f"GET {path}: {error}") from error


def decode_uint64(value: bytes) -> int:
    if len(value) != 8:
        raise ValueError(f"uint64 value must be 8 bytes, got {len(value)}")
    return int.from_bytes(value, "big")


def decode_box_name(name_b64: str) -> dict:
    raw = base64.b64decode(name_b64)
    if len(raw) != 32:
        raise ValueError(f"Pact factory box name must be 32 bytes, got {len(raw)}")
    return {
        "asset_0": decode_uint64(raw[0:8]),
        "asset_1": decode_uint64(raw[8:16]),
        "fee_bps": decode_uint64(raw[16:24]),
        "pool_version": decode_uint64(raw[24:32]),
        "name_b64": name_b64,
    }


def decode_box_value(value_b64: str) -> int:
    raw = base64.b64decode(value_b64)
    if len(raw) != 8:
        raise ValueError(f"Pact factory box value must be 8 bytes, got {len(raw)}")
    return decode_uint64(raw)


def decode_state_key(key_b64: str) -> str:
    return base64.b64decode(key_b64).decode("utf-8")


def state_map(entries: list[dict]) -> dict:
    result = {}
    for entry in entries:
        key = decode_state_key(entry["key"])
        value = entry.get("value", {})
        value_type = value.get("type")
        if value_type == 2:
            result[key] = value.get("uint", 0)
        elif value_type == 1:
            result[key] = base64.b64decode(value.get("bytes", ""))
    return result


def decode_uint64_list(raw: bytes) -> list[int]:
    if len(raw) % 8 != 0:
        raise ValueError(f"uint64 list byte length must align to 8, got {len(raw)}")
    return [decode_uint64(raw[index : index + 8]) for index in range(0, len(raw), 8)]


def read_pool_state(base_url: str, app_id: int, token: str) -> dict:
    app = request_json(base_url, f"/v2/applications/{app_id}", token)
    state = state_map(app.get("params", {}).get("global-state", []))
    config = decode_uint64_list(state.get("CONFIG", b""))
    pool = {
        "state_asset_0": config[0] if len(config) > 0 else None,
        "state_asset_1": config[1] if len(config) > 1 else None,
        "state_fee_bps": config[2] if len(config) > 2 else None,
        "reserve_0": state.get("A"),
        "reserve_1": state.get("B"),
        "total_lp_supply": state.get("L"),
        "lp_asset_id": state.get("LTID"),
        "version": state.get("VERSION"),
    }
    pool["tradable"] = all(
        isinstance(pool[field], int) and pool[field] > 0
        for field in ("reserve_0", "reserve_1", "total_lp_supply")
    )
    return pool


def box_value_path(app_id: int, name_b64: str) -> str:
    query = urllib.parse.urlencode({"name": f"b64:{name_b64}"})
    return f"/v2/applications/{app_id}/box?{query}"


def main() -> int:
    args = parse_args()
    if (args.asset_a is None) != (args.asset_b is None):
        print("ERROR: --asset-a and --asset-b must be provided together", file=sys.stderr)
        return 2

    token = read_token(args.algod_token_file)
    status = request_json(args.algod_url, "/v2/status", token)
    boxes = request_json(
        args.algod_url,
        f"/v2/applications/{args.factory_app_id}/boxes?limit={args.limit}",
        token,
    ).get("boxes", [])

    wanted_pair = None
    if args.asset_a is not None and args.asset_b is not None:
        wanted_pair = tuple(sorted((args.asset_a, args.asset_b)))

    decoded_boxes = []
    skipped = []
    for item in boxes:
        name_b64 = item.get("name", "")
        try:
            pool = decode_box_name(name_b64)
            if wanted_pair and (pool["asset_0"], pool["asset_1"]) != wanted_pair:
                continue
            decoded_boxes.append(pool)
        except Exception as error:  # noqa: BLE001 - diagnostic should report and continue.
            skipped.append({"name_b64": name_b64, "error": str(error)})

    def enrich(pool: dict) -> dict:
        value = request_json(
            args.algod_url,
            box_value_path(args.factory_app_id, pool["name_b64"]),
            token,
        )
        pool["pool_app_id"] = decode_box_value(value["value"])
        if args.include_state:
            pool.update(read_pool_state(args.algod_url, pool["pool_app_id"], token))
        return pool

    pools = []
    workers = max(1, args.workers)
    with ThreadPoolExecutor(max_workers=workers) as executor:
        futures = {executor.submit(enrich, pool): pool for pool in decoded_boxes}
        for future in as_completed(futures):
            pool = futures[future]
            try:
                pools.append(future.result())
            except Exception as error:  # noqa: BLE001 - diagnostic should report and continue.
                skipped.append({"name_b64": pool.get("name_b64", ""), "error": str(error)})

    pools.sort(
        key=lambda pool: (
            pool.get("asset_0", 0),
            pool.get("asset_1", 0),
            pool.get("fee_bps", 0),
            pool.get("pool_app_id", 0),
        )
    )

    report = {
        "network": "testnet",
        "algod_url": args.algod_url,
        "factory_app_id": args.factory_app_id,
        "source_round": status.get("last-round"),
        "box_count": len(boxes),
        "returned_pool_count": len(pools),
        "asset_filter": {
            "asset_0": wanted_pair[0],
            "asset_1": wanted_pair[1],
        }
        if wanted_pair
        else None,
        "pools": pools,
        "skipped": skipped,
    }
    json.dump(report, sys.stdout, indent=2, sort_keys=True)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
