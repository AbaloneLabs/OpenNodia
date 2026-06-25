#!/usr/bin/env python3
"""Print a read-only DEX database consistency summary."""

from __future__ import annotations

import argparse
import json
import sqlite3
from pathlib import Path


def remaining_expr() -> str:
    return "CASE WHEN sell_amount > filled_amount THEN sell_amount - filled_amount ELSE 0 END"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("database", type=Path)
    args = parser.parse_args()

    uri = f"file:{args.database.resolve()}?mode=ro"
    connection = sqlite3.connect(uri, uri=True)
    connection.row_factory = sqlite3.Row

    status_counts = {
        row["status"]: row["count"]
        for row in connection.execute(
            "SELECT status, COUNT(*) AS count FROM orders GROUP BY status"
        )
    }
    trade_count = connection.execute("SELECT COUNT(*) FROM trades").fetchone()[0]
    missing_evidence = connection.execute(
        """
        SELECT COUNT(*)
        FROM orders
        WHERE status IN ('filled', 'cancelled') AND resolution_tx_id IS NULL
        """
    ).fetchone()[0]
    duplicate_escrows = connection.execute(
        """
        SELECT COUNT(*)
        FROM (
            SELECT escrow_addr
            FROM trades
            WHERE escrow_addr IS NOT NULL
            GROUP BY escrow_addr
            HAVING COUNT(*) > 1
        )
        """
    ).fetchone()[0]
    sync_rows = {
        row["key"]: row["value"]
        for row in connection.execute("SELECT key, value FROM sync_state")
    }
    remaining = remaining_expr()
    active_order_directions = [
        dict(row)
        for row in connection.execute(
            f"""
            SELECT sell_asset,
                   buy_asset,
                   COUNT(*) AS active_orders,
                   SUM({remaining}) AS total_remaining_sell_amount,
                   MIN(price) AS min_price,
                   MAX(price) AS max_price
            FROM orders
            WHERE status = 'active' AND {remaining} > 0
            GROUP BY sell_asset, buy_asset
            ORDER BY sell_asset, buy_asset
            """
        )
    ]
    pair_summary = [
        dict(row)
        for row in connection.execute(
            f"""
            SELECT MIN(sell_asset, buy_asset) AS asset_a,
                   MAX(sell_asset, buy_asset) AS asset_b,
                   COUNT(*) AS total_orders,
                   SUM(CASE WHEN status = 'active' AND {remaining} > 0 THEN 1 ELSE 0 END)
                       AS active_orders,
                   SUM(CASE WHEN status = 'active' AND {remaining} > 0 AND sell_asset < buy_asset THEN 1 ELSE 0 END)
                       AS low_to_high_active_orders,
                   SUM(CASE WHEN status = 'active' AND {remaining} > 0 AND sell_asset > buy_asset THEN 1 ELSE 0 END)
                       AS high_to_low_active_orders
            FROM orders
            GROUP BY asset_a, asset_b
            ORDER BY asset_a, asset_b
            """
        )
    ]
    reverse_view_ready = []
    for pair in pair_summary:
        asset_a = int(pair["asset_a"])
        asset_b = int(pair["asset_b"])
        active_orders = int(pair["active_orders"] or 0)
        low_to_high = int(pair["low_to_high_active_orders"] or 0)
        high_to_low = int(pair["high_to_low_active_orders"] or 0)
        if active_orders <= 0:
            status = "missing_active_depth"
        elif low_to_high > 0 and high_to_low > 0:
            status = "bid_ask_depth_ready"
        else:
            status = "one_sided_depth_only"
        reverse_view_ready.append(
            {
                "asset_a": asset_a,
                "asset_b": asset_b,
                "case_hint": (
                    "algo_asa_bid_ask_reverse"
                    if asset_a == 0
                    else "asa_asa_bid_ask_reverse"
                    if asset_b != 0
                    else "asa_algo_bid_ask_reverse"
                ),
                "status": status,
                "active_orders": active_orders,
                "low_to_high_active_orders": low_to_high,
                "high_to_low_active_orders": high_to_low,
            }
        )

    print(
        json.dumps(
            {
                "active_order_directions": active_order_directions,
                "order_status_counts": status_counts,
                "pair_summary": pair_summary,
                "reverse_view_readiness": reverse_view_ready,
                "trade_count": trade_count,
                "resolved_orders_missing_evidence": missing_evidence,
                "duplicate_trade_escrows": duplicate_escrows,
                "last_synced_round": sync_rows.get("last_synced_round", 0),
            },
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
