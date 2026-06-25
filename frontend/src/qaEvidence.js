function textIncludesAny(text, needles) {
  return needles.some((needle) => text.includes(needle));
}

function txIdsFromSubmitResult(result) {
  const values = [];
  if (Array.isArray(result?.tx_ids)) values.push(...result.tx_ids);
  if (result?.tx_id) values.push(result.tx_id);
  if (result?.txid) values.push(result.txid);
  if (Array.isArray(result?.fills)) {
    values.push(...result.fills.map((fill) => fill.tx_id).filter(Boolean));
  }
  if (Array.isArray(result?.result?.tx_ids)) values.push(...result.result.tx_ids);
  if (result?.result?.tx_id) values.push(result.result.tx_id);
  if (result?.result?.txid) values.push(result.result.txid);
  return Array.from(new Set(values.filter(Boolean)));
}

function routeSourceFromCandidate(candidate) {
  return candidate?.source_type || candidate?.source || null;
}

function normalizedRouteSources(source) {
  const text = String(source || '').trim().toLowerCase();
  if (!text) return [];
  if (text === 'orderbook' || text === 'native_orderbook' || text.includes('orderbook')) {
    return ['orderbook'];
  }
  if (
    text === 'native_amm' ||
    text === 'native_pool' ||
    text.includes('native pool') ||
    text.includes('opennodia amm')
  ) {
    return ['native_pool'];
  }
  if (text === 'external_pool' || text === 'external_router') {
    return [text];
  }
  if (text === 'external_tinyman' || text.includes('tinyman')) {
    return ['external_pool', 'tinyman'];
  }
  if (text === 'external_pact' || text.includes('pact')) {
    return ['external_pool', 'pact'];
  }
  return [text];
}

export function routeSourcesFromCandidates(candidates = []) {
  const sources = [];
  for (const candidate of candidates || []) {
    sources.push(...normalizedRouteSources(routeSourceFromCandidate(candidate)));
    sources.push(...normalizedRouteSources(candidate?.source_id));
    sources.push(...normalizedRouteSources(candidate?.source_label));
    for (const leg of candidate?.split_legs || []) {
      sources.push(...normalizedRouteSources(leg?.source_type));
      sources.push(...normalizedRouteSources(leg?.source_id));
      sources.push(...normalizedRouteSources(leg?.source_label));
    }
  }
  return Array.from(new Set(sources));
}

function routeCandidateNetAmount(candidate) {
  const amountOut = Number(candidate?.amount_out || 0);
  if (Number(candidate?.asset_out) === 0) {
    return Math.max(0, amountOut - Number(candidate?.network_fee_microalgo || 0));
  }
  return amountOut;
}

function networkFeeChangesBestRoute(selected, candidates = []) {
  if (!selected || Number(selected.asset_out) !== 0) return false;
  const selectedNet = routeCandidateNetAmount(selected);
  return candidates.some(
    (candidate) =>
      candidate?.route_hash !== selected.route_hash &&
      Number(candidate?.amount_out || 0) > Number(selected.amount_out || 0) &&
      selectedNet > routeCandidateNetAmount(candidate),
  );
}

function routeSourceSetIsOnly(sources, allowed) {
  return sources.length > 0 && sources.every((source) => allowed.includes(source));
}

function routeHasPairReversalDecimals(selected, pairContext = {}) {
  const baseAssetId = Number(pairContext.base_asset_id);
  const quoteAssetId = Number(pairContext.quote_asset_id);
  const baseDecimals = Number(pairContext.base_decimals);
  const quoteDecimals = Number(pairContext.quote_decimals);
  if (
    !Number.isFinite(baseAssetId) ||
    !Number.isFinite(quoteAssetId) ||
    !Number.isFinite(baseDecimals) ||
    !Number.isFinite(quoteDecimals) ||
    baseAssetId === quoteAssetId ||
    baseDecimals === quoteDecimals
  ) {
    return false;
  }
  return Number(selected?.asset_in) === quoteAssetId && Number(selected?.asset_out) === baseAssetId;
}

function routeHasFolksBackedDuplicateEvidence(candidates = [], warnings = []) {
  const text = [
    ...warnings,
    ...candidates.flatMap((candidate) => [
      candidate?.note,
      candidate?.selection_reason,
      candidate?.source_label,
      ...(candidate?.split_legs || []).flatMap((leg) => [leg?.note, leg?.source_label]),
    ]),
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase();
  return text.includes('folks-backed') && text.includes('not counted as an additional amm source');
}

export function routeMatrixCaseHints(kind, result = {}, extra = {}, pairContext = {}) {
  const dex = [];
  const unified = [];

  if (kind === 'route') {
    const fills = result.fills || result.result?.fills || [];
    const createdOrders = result.created_orders || result.result?.created_orders || [];
    const remaining = Number(result.remaining ?? result.result?.remaining ?? 0);
    const candidates = extra.route_candidates || [];
    const sources = routeSourcesFromCandidates(candidates);
    const selected = candidates.find((candidate) => candidate.route_hash === extra.route_hash);
    const selectedSource =
      normalizedRouteSources(extra.source_type || result.source_type || result.result?.source_type)[0] ||
      null;
    const hasSubmittedTx = txIdsFromSubmitResult(result).length > 0;

    if (fills.length === 0 && !hasSubmittedTx) dex.push('ioc_no_match_balance');
    if (fills.length === 1) dex.push('ioc_single_fill_balance');
    if (fills.length > 1) dex.push('ioc_multi_fill_balance');
    if (remaining > 0 && !createdOrders.length) dex.push('ioc_discarded_remainder_balance');
    if (remaining > 0 && createdOrders.length) unified.push('limit_partial_then_standing_remainder');

    if (selectedSource === 'orderbook' && routeSourceSetIsOnly(sources, ['orderbook'])) {
      unified.push('orderbook_only_pair');
    }
    if (selectedSource === 'native_pool' && routeSourceSetIsOnly(sources, ['native_pool'])) {
      unified.push('native_pool_only_pair');
    }
    if (
      selectedSource === 'external_pool' &&
      routeSourceSetIsOnly(sources, ['external_pool', 'tinyman', 'pact'])
    ) {
      unified.push('tinyman_pact_pool_only_pair');
    }
    if (sources.includes('orderbook') && sources.some((source) => source !== 'orderbook')) {
      unified.push('orderbook_amm_price_cross_pair');
    }
    if (networkFeeChangesBestRoute(selected, candidates)) {
      unified.push('network_fee_changes_best_route');
    }
    if (routeHasPairReversalDecimals(selected, pairContext)) {
      unified.push('pair_reversal_decimals');
    }
    if (routeHasFolksBackedDuplicateEvidence(candidates, extra.route_warnings || [])) {
      unified.push('duplicate_folks_backed_pool');
    }
  }

  return {
    dex_additional_matrix: Array.from(new Set(dex.filter(Boolean))),
    unified_routing: Array.from(new Set(unified.filter(Boolean))),
  };
}

export function rejectionMatrixCaseHints(message) {
  const text = String(message || '').toLowerCase();
  const quoteChanged =
    text.includes('refresh the quote') ||
    text.includes('changed since prepare') ||
    text.includes('quote_id no longer matches') ||
    text.includes('route_hash is not present');
  const dex = [];
  const expiryBoundary = textIncludesAny(text, [
    'order expired',
    'expired order',
    'expire round',
    'expire_round',
    'prepared transaction expired',
    'transaction expired at round',
    'lastvalid',
    'last valid',
    'firstvalid',
    'first valid',
    'validity window',
  ]);
  const duplicateSubmit =
    text.includes('duplicate') ||
    textIncludesAny(text, [
      'already submitted',
      'already in ledger',
      'transaction already',
      'txn already',
      'already confirmed',
      'lease already',
    ]);
  const intentReuse =
    !quoteChanged &&
    text.includes('intent') &&
    textIncludesAny(text, [
      'missing',
      'expired',
      'already used',
      'already consumed',
      'consumed',
      'reused',
      'not found',
      'invalid',
    ]);

  if (expiryBoundary) dex.push('expiry_boundary');
  if (duplicateSubmit) dex.push('duplicate_submit');
  if (intentReuse) dex.push('intent_reuse');

  return {
    dex_additional_matrix: dex,
    unified_routing: quoteChanged ? ['quote_then_state_change_rejected'] : [],
  };
}
