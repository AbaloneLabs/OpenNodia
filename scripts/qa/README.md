# DEX QA

The scripts in this directory are non-signing diagnostics. They never accept a
wallet PIN, mnemonic, session token, algod token, or KMD token.

Rules for signed Testnet QA:

- Use the authenticated OpenNodia prepare/submit API from the web UI.
- Enter the PIN only in the browser prompt. Never pass it through argv, an
  environment variable, a shell command, or a log file.
- Record only network, public addresses, asset IDs, transaction IDs, confirmed
  rounds, expected balances, and observed balances.
- Confirm the network and expected fee before every transaction.
- Do not store signed transaction blobs or wallet handles in QA artifacts.

Run the complete non-signing verification suite with:

```sh
./scripts/qa/check.sh
```

Check native AMM guard, math, and mainnet fail-closed coverage without using
wallet secrets:

```sh
python3 scripts/qa/native_amm_readiness.py --run-cargo --api-url http://127.0.0.1:30080
```

Report release-blocking evidence gaps with:

```sh
./scripts/qa/release-readiness.sh
```

Report active DEX plan evidence gaps with:

```sh
python3 scripts/qa/testnet_plan_readiness.py
```

Print machine-readable gaps or operator merge commands with:

```sh
python3 scripts/qa/testnet_plan_readiness.py --json
python3 scripts/qa/testnet_plan_readiness.py --guide
```

After a real browser-signed DEX submit, the DEX screen exposes a non-secret QA
evidence JSON copy button. Save that fragment outside the repo. Current DEX
fragments include `matrix_case_hints`, so merge all hinted readiness cases with:

```sh
python3 scripts/qa/testnet_plan_evidence.py auto-cases \
  --fragment /tmp/dex-submit-fragment.json \
  --evidence "Real browser-signed Testnet IOC single-fill balance check" \
  --run-at-utc 2026-06-20T00:00:00Z
```

When a browser QA run produces several copied fragments, merge all hinted cases
in one pass:

```sh
python3 scripts/qa/testnet_plan_evidence.py fragments \
  --evidence "Real browser-signed Testnet validation batch" \
  /tmp/opennodia-qa/*.json
```

If that same real validation batch covers the orderbook/native/external unified
routing combo, record the combo explicitly at the same time:

```sh
python3 scripts/qa/testnet_plan_evidence.py fragments \
  --combine-unified-combo \
  --evidence "Real browser-signed Testnet validation batch" \
  --combo-evidence "Real browser-signed Testnet route covering orderbook, native AMM, and Tinyman" \
  /tmp/opennodia-qa/*.json
```

The DEX orderbook panel can also copy non-secret view evidence for bid/ask
reverse-view checks. That fragment is still live Testnet evidence, but it does
not require signing because it only records public orderbook snapshots.
`testnet_plan_evidence.py` accepts bid/ask reverse-view cases only when both
the current and reverse views contain real bids and asks. Create active depth
with the browser-signed flow first; empty orderbooks are rejected.

Record the DEX server restart matrix case without wallet secrets:

```sh
python3 scripts/qa/testnet_plan_evidence.py server-restart \
  --api-url http://192.168.0.36:30080/api/status \
  --confirm-restart
```

Record the DEX stale-node/public fallback matrix case from sanitized service
log fields:

```sh
python3 scripts/qa/testnet_plan_evidence.py stale-node-fallback \
  --api-url http://192.168.0.36:30080/api/status
```

For older fragments without hints, merge into a specific plan matrix case:

```sh
python3 scripts/qa/testnet_plan_evidence.py case \
  --section dex_additional_matrix \
  --case ioc_single_fill_balance \
  --fragment /tmp/dex-submit-fragment.json \
  --evidence "Real browser-signed Testnet IOC single-fill balance check" \
  --run-at-utc 2026-06-20T00:00:00Z

python3 scripts/qa/testnet_plan_evidence.py unified-combo \
  --fragment /tmp/router-submit-fragment.json \
  --evidence "Real browser-signed Testnet route covering orderbook, native AMM, and Tinyman" \
  --run-at-utc 2026-06-20T00:00:00Z
```

`unified-combo` infers sources from `sources_seen`, `source_type`, and copied
route candidates. Use `--sources orderbook,native_pool,tinyman` only when the
fragment came from an older UI that did not include source metadata.

Set `OPENNODIA_PLAN_READINESS_STRICT=true` only after the real
`unified_routing` and `dex_additional_matrix` signed Testnet evidence has been
recorded. The helper reports missing evidence; it does not replace the browser
PIN-gated validation run.

The readiness script reads `scripts/qa/testnet-evidence.json` and
`scripts/qa/public-release-evidence.json`. It reports missing Pact live
liquidity evidence, cross-platform install evidence, release signing evidence,
external review status, and upgrade/rollback evidence. Set
`OPENNODIA_RELEASE_READINESS_STRICT=true` for public release gating.
The public release evidence shape is documented in
`scripts/qa/public-release-evidence.schema.json`. Do not create the actual
evidence file with placeholder values; record only real validation results and
links to non-secret run or review artifacts.

Use the public release evidence helper to create non-secret fragments after
each real validation run. The helper refuses to mark install, reboot, ARM64
sync/restart, or upgrade/rollback checks as passed unless the matching
confirmation flag is provided by the operator who ran the validation:

```sh
python3 scripts/qa/public_release_evidence.py desktop \
  --platform macos \
  --run-url https://github.com/OWNER/REPO/actions/runs/RUN_ID \
  --confirm-install-upgrade-reboot \
  --output /tmp/macos-evidence.json

python3 scripts/qa/public_release_evidence.py linux-arm64 \
  --run-url https://example.com/opennodia/qa/linux-arm64-run \
  --api-url http://127.0.0.1:30080 \
  --compose-dir /opt/opennodia \
  --compose-service opennodia \
  --confirm-full-stack-sync-restart \
  --output /tmp/arm64-evidence.json

python3 scripts/qa/public_release_evidence.py merge \
  --output scripts/qa/public-release-evidence.json \
  --complete \
  /tmp/macos-evidence.json /tmp/windows-evidence.json /tmp/arm64-evidence.json
```

Inspect a copied or stopped DEX database without modifying it with:

```sh
python3 scripts/qa/dex_db_report.py /path/to/dex.sqlite
```

The report includes active order directions, canonical pair summaries, and
`reverse_view_readiness` so operators can tell whether bid/ask reverse-view
evidence can be recorded before opening the browser QA flow.

Discover real Pact Testnet pool candidates without signing transactions with:

```sh
python3 scripts/qa/pact_factory_report.py --include-state --workers 16
```

Filter to a pair before running live Pact QA:

```sh
python3 scripts/qa/pact_factory_report.py --asset-a 0 --asset-b 14704676 --include-state
```
