use super::*;

/// `GET /api/dex/orderbook` — orderbook snapshot for a pair.
pub(super) async fn orderbook(
    State(state): State<AppState>,
    Query(q): Query<OrderbookQuery>,
) -> ApiResult<Json<OrderbookResponse>> {
    let db = require_dex(&state)?;
    let pair = Pair::new(q.asset_a, q.asset_b);

    let (_, status, source) = state
        .authoritative_ledger()
        .await
        .map_err(service_unavailable)?;
    let current_round = status.last_round;

    let snapshot = opennodia_dex::get_orderbook(&db, pair, q.asset_a, current_round)
        .map_err(|e| internal(format!("orderbook: {e}")))?;

    let (synthetic_bids, synthetic_asks, synthetic_warnings) =
        synthetic_orderbook_depth(&state, &snapshot, q.asset_a, q.depth).await;
    let mut resp = OrderbookResponse::from_snapshot(snapshot, source);
    resp.synthetic_bids = synthetic_bids;
    resp.synthetic_asks = synthetic_asks;
    resp.warnings = synthetic_warnings;
    // Apply depth limit.
    resp.bids.truncate(q.depth as usize);
    resp.asks.truncate(q.depth as usize);
    Ok(Json(resp))
}

/// `GET /api/dex/pairs` — popular trading pairs ranked by activity.
///
/// Combines active order counts and recent trade frequency/volume into a
/// composite score so the UI can render a "popular pairs" sidebar. Pairs with
/// no activity are omitted; callers that want a guaranteed non-empty list
/// should merge these results with the caller's held assets.
pub(super) async fn popular_pairs(
    State(state): State<AppState>,
    Query(q): Query<PairsQuery>,
) -> ApiResult<Json<PairsResponse>> {
    let db = require_dex(&state)?;

    let (_, status, source) = state
        .authoritative_ledger()
        .await
        .map_err(service_unavailable)?;
    let recent_trade_round = Round(status.last_round.as_u64().saturating_sub(q.recent_rounds));

    let stats = db
        .get_pair_stats(recent_trade_round, q.limit)
        .map_err(|e| internal(format!("pair stats: {e}")))?;

    Ok(Json(PairsResponse {
        pairs: stats.into_iter().map(Into::into).collect(),
        source_round: status.last_round.as_u64(),
        source,
    }))
}

/// `GET /api/dex/status` — runtime write gate and reconciliation status.
pub(super) async fn dex_status(
    State(state): State<AppState>,
) -> ApiResult<Json<DexStatusResponse>> {
    let db = require_dex(&state)?;
    let validation = state.runtime.dex_validation.snapshot();
    Ok(Json(DexStatusResponse {
        write_enabled: validation.allows_writes(),
        validation,
        active_orders: db
            .count_active_orders()
            .map_err(|error| internal(format!("count active orders: {error}")))?,
        last_reconciled_round: db
            .get_last_synced_round()
            .map_err(|error| internal(format!("read reconciliation round: {error}")))?,
    }))
}

/// `GET /api/dex/orders` — the caller's orders.
pub(super) async fn my_orders(
    State(state): State<AppState>,
    Query(q): Query<MyOrdersQuery>,
) -> ApiResult<Json<MyOrdersResponse>> {
    let db = require_dex(&state)?;
    let owners = resolve_order_owners(&state, &q.wallet_id).await?;

    let status_filter = match q.status.as_str() {
        "active" => Some(EntryStatus::Active),
        "filled" => Some(EntryStatus::Filled),
        "cancelled" => Some(EntryStatus::Cancelled),
        "expired" => Some(EntryStatus::Expired),
        "closed_unresolved" => Some(EntryStatus::ClosedUnresolved),
        "all" => None,
        other => {
            return Err(bad_request(format!(
                "invalid status '{other}'; expected active|filled|cancelled|expired|closed_unresolved|all"
            )))
        }
    };

    let mut orders = Vec::new();
    for owner in owners {
        orders.extend(
            db.get_orders_for_owner(&owner, status_filter)
                .map_err(|e| internal(format!("query orders: {e}")))?,
        );
    }
    orders.sort_by(|left, right| {
        right
            .created_round
            .as_u64()
            .cmp(&left.created_round.as_u64())
    });

    Ok(Json(MyOrdersResponse {
        orders: orders.into_iter().map(Into::into).collect(),
    }))
}

/// `GET /api/dex/trades` — recent trades.
pub(super) async fn trades(
    State(state): State<AppState>,
    Query(q): Query<TradesQuery>,
) -> ApiResult<Json<TradesResponse>> {
    let db = require_dex(&state)?;

    let (trades, view_base_asset) = if let Some(addr_str) = &q.address {
        let addr = parse_address(addr_str)?;
        (
            db.get_trades_for_account(&addr, q.limit)
                .map_err(|e| internal(format!("trades: {e}")))?,
            None,
        )
    } else if let Some(pair_str) = &q.pair {
        let (a, b) = parse_pair_str(pair_str)?;
        let pair = Pair::new(a, b);
        (
            db.get_recent_trades(pair, q.limit)
                .map_err(|e| internal(format!("trades: {e}")))?,
            Some(a),
        )
    } else {
        return Err(bad_request(
            "provide either 'pair' (a:b) or 'address' query parameter",
        ));
    };

    Ok(Json(TradesResponse {
        trades: trades
            .into_iter()
            .map(|trade| TradeResponse::from_view(trade, view_base_asset))
            .collect(),
    }))
}

/// `GET /api/dex/order/:escrow/link` — shareable link for a local order.
pub(super) async fn order_link_for_order(
    State(state): State<AppState>,
    Path(escrow_str): Path<String>,
) -> ApiResult<Json<OrderLinkGenerateResponse>> {
    let db = require_dex(&state)?;
    let escrow_addr = parse_address(&escrow_str)?;
    let entry = db
        .get_order(&escrow_addr)
        .map_err(|error| internal(format!("get order: {error}")))?
        .ok_or_else(|| not_found(format!("order not found: {escrow_str}")))?;

    let (algod, _, _) = state
        .authoritative_ledger()
        .await
        .map_err(service_unavailable)?;
    canonical_escrow_from_entry(algod, &entry).await?;

    Ok(Json(order_link_response_from_payload(
        order_link_payload_from_entry(&entry),
    )?))
}

/// `GET /api/dex/order-link/:payload` — decoded order link + on-chain verification.
pub(super) async fn order_link_detail(
    State(state): State<AppState>,
    Path(payload_str): Path<String>,
) -> ApiResult<Json<OrderLinkDetailResponse>> {
    let payload = decode_order_link(&payload_str)
        .map_err(|error| bad_request(format!("invalid order link: {error}")))?;
    let (algod, status, source) = state
        .authoritative_ledger()
        .await
        .map_err(service_unavailable)?;
    let decoded = (&payload).into();
    let payload_escrow = payload.escrow_address();
    let fallback_url = format!("/#/dex/order/{payload_str}");

    let params = EscrowParams::new(
        payload.owner_address(),
        payload.sell_asset,
        payload.sell_amount,
        payload.buy_asset,
        payload.buy_amount,
        payload.expire_round,
    );
    let canonical = match EscrowAccount::compile(algod, payload.escrow_kind(), params).await {
        Ok(escrow) => escrow,
        Err(error) => {
            return Ok(Json(OrderLinkDetailResponse {
                payload: payload_str,
                url: fallback_url,
                decoded,
                payload_valid: false,
                canonical_escrow_match: false,
                canonical_escrow_address: None,
                status: "invalid_payload".to_string(),
                order: None,
                resolution: None,
                verification: None,
                source_round: status.last_round.as_u64(),
                source,
                error: Some(format!("compile canonical escrow: {error}")),
            }));
        }
    };

    let canonical_address = canonical.address;
    let canonical_escrow_match = canonical_address == payload_escrow;
    let mut local_order = None;
    if let Some(db) = state.stores.dex.as_ref() {
        local_order = db
            .get_order(&payload_escrow)
            .map_err(|error| internal(format!("get linked order: {error}")))?;
    }
    let order_status = local_order
        .as_ref()
        .map(|entry| entry.status.as_str().to_string());
    if !canonical_escrow_match {
        return Ok(Json(OrderLinkDetailResponse {
            payload: payload_str,
            url: fallback_url,
            decoded,
            payload_valid: false,
            canonical_escrow_match: false,
            canonical_escrow_address: Some(canonical_address.to_string()),
            status: "invalid_payload".to_string(),
            order: local_order.map(Into::into),
            resolution: None,
            verification: None,
            source_round: status.last_round.as_u64(),
            source,
            error: Some("payload escrow address does not match canonical parameters".to_string()),
        }));
    }

    let verification = match algod
        .account_info_optional(&canonical.address.to_string())
        .await
    {
        Ok(Some(info)) => opennodia_swap::verify::verify_escrow_with_info(
            &canonical,
            &info,
            status.last_round,
            source,
        ),
        Ok(None) => OrderVerification::failed("escrow account not found on ledger", source),
        Err(error) => {
            return Ok(Json(OrderLinkDetailResponse {
                payload: payload_str,
                url: fallback_url,
                decoded,
                payload_valid: true,
                canonical_escrow_match: true,
                canonical_escrow_address: Some(canonical_address.to_string()),
                status: "ledger_unverified".to_string(),
                order: local_order.map(Into::into),
                resolution: None,
                verification: None,
                source_round: status.last_round.as_u64(),
                source,
                error: Some(format!("verify linked escrow: {error}")),
            }));
        }
    };
    let mut resolution = None;
    if local_order.is_none() && !verification.valid {
        let linked_entry = order_entry_from_link_payload(&payload, &canonical)?;
        if let Some((indexer, _)) = state.effective_search_client().await {
            match opennodia_dex::resolve_closed_order(indexer, &linked_entry).await {
                Ok(Some(opennodia_dex::EscrowEvent::Filled(trade))) => {
                    resolution = Some(OrderLinkResolutionResponse {
                        status: "filled_external".to_string(),
                        tx_id: Some(trade.tx_id),
                        round: Some(trade.round.as_u64()),
                    });
                }
                Ok(Some(opennodia_dex::EscrowEvent::Cancelled { tx_id, round })) => {
                    resolution = Some(OrderLinkResolutionResponse {
                        status: "cancelled_external".to_string(),
                        tx_id: Some(tx_id),
                        round: Some(round.as_u64()),
                    });
                }
                Ok(Some(opennodia_dex::EscrowEvent::Expired)) => {
                    resolution = Some(OrderLinkResolutionResponse {
                        status: "expired_external".to_string(),
                        tx_id: None,
                        round: None,
                    });
                }
                Ok(Some(opennodia_dex::EscrowEvent::ClosedUnresolved { round })) => {
                    resolution = Some(OrderLinkResolutionResponse {
                        status: "closed_unresolved_external".to_string(),
                        tx_id: None,
                        round: Some(round.as_u64()),
                    });
                }
                Ok(Some(opennodia_dex::EscrowEvent::Unchanged)) | Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, escrow = %canonical.address, "linked order resolution failed");
                }
            }
        }
    }

    let status_label = order_status
        .or_else(|| resolution.as_ref().map(|item| item.status.clone()))
        .unwrap_or_else(|| {
            if verification.valid {
                "active_external".to_string()
            } else if verification.expired {
                "expired_external".to_string()
            } else {
                "invalid_external".to_string()
            }
        });

    Ok(Json(OrderLinkDetailResponse {
        payload: payload_str,
        url: fallback_url,
        decoded,
        payload_valid: true,
        canonical_escrow_match: true,
        canonical_escrow_address: Some(canonical_address.to_string()),
        status: status_label,
        order: local_order.map(Into::into),
        resolution,
        verification: Some(verification.into()),
        source_round: status.last_round.as_u64(),
        source,
        error: None,
    }))
}

/// `GET /api/dex/order/:escrow` — single order + on-chain verification.
pub(super) async fn order_detail(
    State(state): State<AppState>,
    Path(escrow_str): Path<String>,
) -> ApiResult<Json<OrderDetailResponse>> {
    let db = require_dex(&state)?;
    let escrow_addr = parse_address(&escrow_str)?;

    let entry = db
        .get_order(&escrow_addr)
        .map_err(|e| internal(format!("get order: {e}")))?;

    let entry = entry.ok_or_else(|| not_found(format!("order not found: {escrow_str}")))?;
    let (algod, status, source) = state
        .authoritative_ledger()
        .await
        .map_err(service_unavailable)?;
    let escrow = canonical_escrow_from_entry(algod, &entry).await?;
    let verification = verify_escrow(algod, None, &escrow, status.last_round)
        .await
        .map_err(|error| service_unavailable(format!("verify escrow: {error}")))?;

    Ok(Json(OrderDetailResponse {
        order: Some(entry.into()),
        verification: Some(verification.into()),
        source_round: status.last_round.as_u64(),
        source,
    }))
}

/// Parse a "a:b" pair string.
pub(super) fn parse_pair_str(s: &str) -> ApiResult<(u64, u64)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(bad_request(format!(
            "invalid pair '{s}'; expected 'asset_a:asset_b'"
        )));
    }
    let a = parts[0]
        .parse::<u64>()
        .map_err(|e| bad_request(format!("invalid asset_a: {e}")))?;
    let b = parts[1]
        .parse::<u64>()
        .map_err(|e| bad_request(format!("invalid asset_b: {e}")))?;
    Ok((a, b))
}
