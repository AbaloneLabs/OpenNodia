#!/bin/sh
# Run the complete non-signing DEX verification suite.

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "${SCRIPT_DIR}/../.." && pwd)

cd "$REPO_ROOT"
if [ "${OPENNODIA_QA_SKIP_RUST:-0}" != "1" ]; then
    cargo fmt --all -- --check
    cargo test --workspace --all-targets --locked
    cargo clippy --workspace --all-targets --locked -- -D warnings
fi
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
    scripts/qa/dex_db_report.py \
    scripts/qa/native_amm_readiness.py \
    scripts/qa/pact_factory_report.py \
    scripts/qa/public_release_evidence.py \
    scripts/qa/testnet_plan_evidence.py \
    scripts/qa/testnet_plan_readiness.py
python3 scripts/qa/native_amm_readiness.py --help | grep -q -- "native AMM guard"
python3 scripts/qa/native_amm_readiness.py
python3 scripts/qa/testnet_plan_evidence.py --help | grep -q -- "signed Testnet QA fragments"
python3 scripts/qa/testnet_plan_evidence.py auto-cases --help | grep -q -- "matrix_case_hints"
python3 scripts/qa/testnet_plan_evidence.py fragments --help | grep -q -- "--combine-unified-combo"
python3 scripts/qa/testnet_plan_evidence.py server-restart --help | grep -q -- "--confirm-restart"
python3 scripts/qa/testnet_plan_evidence.py stale-node-fallback --help | grep -q -- "--log-tail"
python3 scripts/qa/testnet_plan_readiness.py --help | grep -q -- "--guide"
python3 scripts/qa/public_release_evidence.py linux-arm64 --help | grep -q -- "--compose-service"
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT
printf '{ "network": "testnet" }\n' > "$tmp_dir/auto-evidence.json"
cat > "$tmp_dir/auto-fragment.json" <<'JSON'
{
  "kind": "orderbook_view",
  "network": "testnet",
  "wallet_address": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY5HFKQ",
  "base_asset_id": 0,
  "quote_asset_id": 42,
  "tx_id": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
  "confirmed_round": 123,
  "current_view": {
    "view_asset_id": 0,
    "bids": [{"price": 1000, "amount": 10}],
    "asks": [{"price": 1100, "amount": 10}]
  },
  "reverse_view": {
    "view_asset_id": 42,
    "bids": [{"price": 900, "amount": 10}],
    "asks": [{"price": 1000, "amount": 10}]
  },
  "matrix_case_hints": {
    "dex_additional_matrix": ["ioc_single_fill_balance", "algo_asa_bid_ask_reverse"],
    "unified_routing": ["orderbook_only_pair"]
  }
}
JSON
python3 scripts/qa/testnet_plan_evidence.py auto-cases \
    --fragment "$tmp_dir/auto-fragment.json" \
    --output "$tmp_dir/auto-evidence.json" \
    --evidence "QA helper self-test fragment" \
    --run-at-utc 2026-06-20T00:00:00Z
python3 - "$tmp_dir/auto-evidence.json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert data["dex_additional_matrix"]["matrix"]["ioc_single_fill_balance"]["passed"] is True
assert data["dex_additional_matrix"]["matrix"]["algo_asa_bid_ask_reverse"]["passed"] is True
assert data["unified_routing"]["matrix"]["orderbook_only_pair"]["passed"] is True
assert data["dex_additional_matrix"]["confirmed_tx_ids"] == [
    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
]
assert data["dex_additional_matrix"]["wallet_addresses"] == [
    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY5HFKQ"
]
PY
cat > "$tmp_dir/empty-orderbook-fragment.json" <<'JSON'
{
  "kind": "orderbook_view",
  "network": "testnet",
  "wallet_address": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY5HFKQ",
  "base_asset_id": 0,
  "quote_asset_id": 42,
  "current_view": {
    "view_asset_id": 0,
    "bids": [],
    "asks": []
  },
  "reverse_view": {
    "view_asset_id": 42,
    "bids": [],
    "asks": []
  },
  "matrix_case_hints": {
    "dex_additional_matrix": ["algo_asa_bid_ask_reverse"],
    "unified_routing": []
  }
}
JSON
if python3 scripts/qa/testnet_plan_evidence.py auto-cases \
    --fragment "$tmp_dir/empty-orderbook-fragment.json" \
    --output "$tmp_dir/auto-evidence.json" \
    --evidence "QA helper empty orderbook fragment" \
    --run-at-utc 2026-06-20T00:00:00Z >/dev/null 2>&1; then
    echo "ERROR: orderbook view evidence without bid/ask depth was accepted" >&2
    exit 1
fi
printf '{ "network": "testnet" }\n' > "$tmp_dir/batch-evidence.json"
cat > "$tmp_dir/batch-orderbook-native.json" <<'JSON'
{
  "network": "testnet",
  "run_at_utc": "2026-06-20T00:00:01Z",
  "wallet_address": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAY5HFKQ",
  "tx_id": "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
  "confirmed_round": 124,
  "sources_seen": ["orderbook", "native_pool"],
  "matrix_case_hints": {
    "dex_additional_matrix": [],
    "unified_routing": ["native_pool_only_pair"]
  }
}
JSON
cat > "$tmp_dir/batch-external.json" <<'JSON'
{
  "network": "testnet",
  "run_at_utc": "2026-06-20T00:00:02Z",
  "wallet_address": "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBY7YB",
  "tx_id": "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC",
  "confirmed_round": 125,
  "source_label": "Tinyman",
  "matrix_case_hints": {
    "dex_additional_matrix": [],
    "unified_routing": ["tinyman_pact_pool_only_pair"]
  }
}
JSON
python3 scripts/qa/testnet_plan_evidence.py fragments \
    --output "$tmp_dir/batch-evidence.json" \
    --evidence "QA helper batch self-test fragment" \
    --combine-unified-combo \
    --combo-evidence "QA helper batch self-test combo" \
    "$tmp_dir/batch-orderbook-native.json" \
    "$tmp_dir/batch-external.json"
python3 - "$tmp_dir/batch-evidence.json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert data["unified_routing"]["matrix"]["native_pool_only_pair"]["passed"] is True
assert data["unified_routing"]["matrix"]["tinyman_pact_pool_only_pair"]["passed"] is True
combo = data["unified_routing"]["orderbook_native_external_combo"]
assert combo["passed"] is True
assert {"orderbook", "native_pool", "tinyman"}.issubset(set(combo["sources_seen"]))
assert combo["confirmed_tx_ids"] == [
    "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
    "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC",
]
assert combo["source_round"] == 125
PY
python3 - "$tmp_dir/dex-report.sqlite" <<'PY'
import sqlite3
import sys

path = sys.argv[1]
connection = sqlite3.connect(path)
connection.executescript(
    """
    CREATE TABLE orders (
        escrow_addr TEXT PRIMARY KEY,
        side TEXT NOT NULL,
        sell_asset INTEGER NOT NULL,
        sell_amount INTEGER NOT NULL,
        buy_asset INTEGER NOT NULL,
        buy_amount INTEGER NOT NULL,
        price INTEGER NOT NULL,
        owner TEXT NOT NULL,
        created_round INTEGER NOT NULL,
        expire_round INTEGER NOT NULL,
        status TEXT NOT NULL DEFAULT 'active',
        filled_amount INTEGER NOT NULL DEFAULT 0,
        split_index INTEGER NOT NULL DEFAULT 0,
        parent_id TEXT,
        program TEXT,
        params TEXT,
        resolution_tx_id TEXT,
        resolution_round INTEGER
    );
    CREATE TABLE trades (
        tx_id TEXT PRIMARY KEY,
        pair_a INTEGER NOT NULL,
        pair_b INTEGER NOT NULL,
        side TEXT NOT NULL,
        price INTEGER NOT NULL,
        amount INTEGER NOT NULL,
        buyer TEXT NOT NULL,
        seller TEXT NOT NULL,
        round INTEGER NOT NULL,
        timestamp INTEGER NOT NULL,
        base_asset INTEGER,
        escrow_addr TEXT
    );
    CREATE TABLE sync_state (key TEXT PRIMARY KEY, value INTEGER NOT NULL);
    INSERT INTO sync_state (key, value) VALUES ('last_synced_round', 1234);
    INSERT INTO orders (
        escrow_addr, side, sell_asset, sell_amount, buy_asset, buy_amount, price,
        owner, created_round, expire_round, status, filled_amount
    ) VALUES
        ('escrow-a', 'sell', 0, 1000, 42, 10, 10000, 'owner-a', 1, 100, 'active', 0),
        ('escrow-b', 'buy', 42, 10, 0, 1000, 10000, 'owner-b', 1, 100, 'active', 0),
        ('escrow-c', 'sell', 0, 1000, 42, 10, 10000, 'owner-c', 1, 100, 'filled', 1000);
    INSERT INTO trades (
        tx_id, pair_a, pair_b, side, price, amount, buyer, seller, round, timestamp, escrow_addr
    ) VALUES ('trade-a', 0, 42, 'sell', 10000, 10, 'buyer', 'seller', 2, 3, 'escrow-c');
    """
)
connection.commit()
PY
python3 scripts/qa/dex_db_report.py "$tmp_dir/dex-report.sqlite" >"$tmp_dir/dex-report.json"
python3 - "$tmp_dir/dex-report.json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert data["last_synced_round"] == 1234
assert data["order_status_counts"] == {"active": 2, "filled": 1}
assert data["active_order_directions"][0]["sell_asset"] == 0
assert data["reverse_view_readiness"][0]["status"] == "bid_ask_depth_ready"
assert data["reverse_view_readiness"][0]["case_hint"] == "algo_asa_bid_ask_reverse"
PY
if ! python3 scripts/qa/testnet_plan_readiness.py >"$tmp_dir/plan-readiness.out" 2>&1; then
    cat "$tmp_dir/plan-readiness.out" >&2
    echo "ERROR: Testnet plan readiness could not parse the evidence file" >&2
    exit 1
fi
python3 scripts/qa/testnet_plan_readiness.py --json >"$tmp_dir/plan-readiness.json"
python3 - "$tmp_dir/plan-readiness.json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert isinstance(data["complete"], bool)
assert data["sections"]["unified_routing"]["combo_status"] in {"missing", "incomplete", "passed"}
if data["complete"]:
    assert data["sections"]["unified_routing"]["missing_cases"] == []
else:
    assert any(
        item["case"] == "network_fee_changes_best_route"
        for item in data["sections"]["unified_routing"]["missing_cases"]
    )
PY
python3 scripts/qa/testnet_plan_readiness.py --guide >"$tmp_dir/plan-readiness-guide.out"
grep -q -- "testnet_plan_evidence.py fragments" "$tmp_dir/plan-readiness-guide.out"
grep -q -- "unified-combo" "$tmp_dir/plan-readiness-guide.out"
(
    cd "$tmp_dir"
    "$REPO_ROOT/scripts/qa/release-readiness.sh" >readiness-from-outside.out 2>&1
)
if grep -q "Testnet evidence file is missing" "$tmp_dir/readiness-from-outside.out"; then
    cat "$tmp_dir/readiness-from-outside.out" >&2
    echo "ERROR: release readiness did not resolve default evidence paths from the repository root" >&2
    exit 1
fi
python3 scripts/qa/public_release_evidence.py external-review \
    --reviewer QA \
    --report-url https://gitlab.opennodia.local/abalonelabs/opennodia/-/pipelines/1 \
    --completed-at-utc 2026-06-20T00:00:00Z \
    --blockers-open 0 \
    --output "$tmp_dir/review.json"
python3 scripts/qa/public_release_evidence.py validate "$tmp_dir/review.json"
python3 - "$tmp_dir/review.json" "$tmp_dir/review-extra.json" <<'PY'
import json
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
data = json.loads(source.read_text(encoding="utf-8"))
data["external_review"]["unexpected"] = "reject"
target.write_text(json.dumps(data), encoding="utf-8")
PY
if python3 scripts/qa/public_release_evidence.py validate "$tmp_dir/review-extra.json" >/dev/null 2>&1; then
    echo "ERROR: public release evidence validation accepted an unexpected key" >&2
    exit 1
fi
printf '[]\n' > "$tmp_dir/not-object.json"
if python3 scripts/qa/public_release_evidence.py validate --complete "$tmp_dir/not-object.json" >/dev/null 2>&1; then
    echo "ERROR: public release evidence validation accepted a non-object document" >&2
    exit 1
fi
if OPENNODIA_RELEASE_READINESS_STRICT=true \
    ./scripts/qa/release-readiness.sh scripts/qa/testnet-evidence.json "$tmp_dir/review-extra.json" \
    >"$tmp_dir/readiness.out" 2>&1; then
    echo "ERROR: release readiness accepted public evidence with an unexpected key" >&2
    exit 1
fi
if ! grep -q "Public release evidence schema validation must pass" "$tmp_dir/readiness.out"; then
    cat "$tmp_dir/readiness.out" >&2
    echo "ERROR: release readiness did not report public evidence schema validation failure" >&2
    exit 1
fi
printf '{\n' > "$tmp_dir/invalid.json"
if python3 scripts/qa/public_release_evidence.py validate "$tmp_dir/invalid.json" >/dev/null 2>&1; then
    echo "ERROR: public release evidence validation accepted invalid JSON" >&2
    exit 1
fi
if python3 scripts/qa/public_release_evidence.py merge \
    --output "$tmp_dir/merged.json" \
    "$tmp_dir/not-object.json" >/dev/null 2>&1; then
    echo "ERROR: public release evidence merge accepted a non-object fragment" >&2
    exit 1
fi

if [ "${OPENNODIA_QA_SKIP_FRONTEND:-0}" != "1" ]; then
    cd "$REPO_ROOT/frontend"
    npm test
    npm audit --audit-level=moderate
    npm run build
fi

cd "$REPO_ROOT"
if [ "${OPENNODIA_QA_SKIP_SECRETS:-0}" != "1" ]; then
    ./scripts/check-secrets.sh
fi
