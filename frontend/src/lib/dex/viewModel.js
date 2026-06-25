import {
  multiplyDecimalsToRaw,
  parseDecimalToRaw,
  rawToSafeNumber,
} from '../../amount.js';

export const EXPIRY_PRESETS = [
  { id: 'instant', seconds: 10, labelKey: 'dex.expiryInstant' },
  { id: '1h', seconds: 3600, labelKey: 'dex.expiry1h' },
  { id: '6h', seconds: 6 * 3600, labelKey: 'dex.expiry6h' },
  { id: '1d', seconds: 86400, labelKey: 'dex.expiry1d' },
  { id: '7d', seconds: 7 * 86400, labelKey: 'dex.expiry7d' },
  { id: 'max', seconds: null, labelKey: 'dex.expiryMax' },
];

export const CHART_RANGES = [
  { id: '1h', seconds: 3600, labelKey: 'dex.chart1h' },
  { id: '1d', seconds: 86400, labelKey: 'dex.chart1d' },
  { id: '1w', seconds: 7 * 86400, labelKey: 'dex.chart1w' },
  { id: 'all', seconds: null, labelKey: 'dex.chartAll' },
];

export const SECONDS_PER_ROUND = 3.3;
const EMPTY_DISPLAY = '\u2014';

export function roundsForSeconds(seconds) {
  if (seconds == null) return 1_000_000;
  return Math.max(3, Math.round(seconds / SECONDS_PER_ROUND));
}

export function presetIdForRounds(rounds) {
  return EXPIRY_PRESETS.find((preset) => roundsForSeconds(preset.seconds) === rounds)?.id || '';
}

export function estimatedExpiryText(rounds, translate) {
  const seconds = Math.max(0, Number(rounds) || 0) * SECONDS_PER_ROUND;
  if (seconds < 60) {
    return translate('dex.estimatedExpirySeconds', { count: Math.max(0, Math.round(seconds)) });
  }
  if (seconds < 3600) {
    return translate('dex.estimatedExpiryMinutes', { count: Math.round(seconds / 60) });
  }
  if (seconds < 86400) {
    return translate('dex.estimatedExpiryHours', { count: Math.round(seconds / 3600) });
  }
  return translate('dex.estimatedExpiryDays', { count: Math.round(seconds / 86400) });
}

export function splitGroupOrders(orders, order) {
  if (!order?.parent_id) return [order].filter(Boolean);
  return (orders || [])
    .filter((item) => item.parent_id === order.parent_id)
    .sort((left, right) => Number(left.split_index || 0) - Number(right.split_index || 0));
}

export function splitActiveOrders(orders, order) {
  return splitGroupOrders(orders, order).filter((item) => item.status === 'active');
}

export function splitChildCount(orders, order) {
  return splitGroupOrders(orders, order).length;
}

export function splitProgress(orders, order) {
  const group = splitGroupOrders(orders, order);
  const total = group.reduce((sum, item) => sum + Number(item.sell_amount || 0), 0);
  const filled = group.reduce((sum, item) => sum + Number(item.filled_amount || 0), 0);
  const pct = total > 0 ? Math.round((filled / total) * 100) : 0;
  return { total, filled, pct };
}

export function isFirstActiveSplitChild(orders, order) {
  if (!order?.parent_id || order.status !== 'active') return false;
  return splitActiveOrders(orders, order)[0]?.escrow_addr === order.escrow_addr;
}

export function cancelBatchRecoverableAlgo(prepares) {
  return (prepares || []).reduce((sum, item) => sum + Number(item.prepare?.recoverable_algo || 0), 0);
}

export function cancelBatchRecoverableAssets(prepares) {
  const totals = new Map();
  for (const item of prepares || []) {
    const asset = item.prepare?.recoverable_asset;
    if (!asset) continue;
    const [assetId, amount] = asset;
    totals.set(assetId, (totals.get(assetId) || 0) + Number(amount || 0));
  }
  return Array.from(totals.entries()).sort((left, right) => Number(left[0]) - Number(right[0]));
}

export function compactOrderbookLevels(levels = []) {
  return levels.slice(0, 5).map((level) => ({
    price: level.price,
    amount: level.amount,
    total: level.total,
    order_count: level.order_count,
  }));
}

export function compactSyntheticLevels(levels = []) {
  return levels.slice(0, 5).map((level) => ({
    price: level.price,
    amount: level.amount,
    total: level.total,
    source: level.source,
    source_label: level.source_label,
    pool_id: level.pool_id,
    app_id: level.app_id,
    fee_bps: level.fee_bps,
    price_impact_bps: level.price_impact_bps,
    executable: level.executable,
    source_round: level.source_round,
  }));
}

export function orderbookViewEvidenceSnapshot(snapshot, viewAssetId) {
  return {
    view_asset_id: Number(viewAssetId),
    pair: snapshot?.pair || null,
    source: snapshot?.source || null,
    last_update_round: snapshot?.last_update_round || null,
    spread: snapshot?.spread ?? null,
    last_price: snapshot?.last_price ?? null,
    bids: compactOrderbookLevels(snapshot?.bids || []),
    asks: compactOrderbookLevels(snapshot?.asks || []),
    synthetic_bids: compactSyntheticLevels(snapshot?.synthetic_bids || []),
    synthetic_asks: compactSyntheticLevels(snapshot?.synthetic_asks || []),
    warnings: snapshot?.warnings || [],
  };
}

export function assetRawBalanceFromAccount(account, assetId) {
  if (!account) return 0;
  if (Number(assetId) === 0) return account.amount || 0;
  const holding = (account.assets || []).find((asset) => Number(asset['asset-id']) === Number(assetId));
  return holding ? holding.amount : 0;
}

export function deriveOrderParams({
  side,
  amount,
  price,
  baseAssetId,
  quoteAssetId,
  baseDecimals,
  quoteDecimals,
}) {
  const baseAmount = rawToSafeNumber(parseDecimalToRaw(amount, baseDecimals));
  const quoteAmount = rawToSafeNumber(multiplyDecimalsToRaw(amount, price, quoteDecimals));
  if (side === 'buy') {
    return [Number(quoteAssetId || 0), quoteAmount, Number(baseAssetId || 0), baseAmount];
  }
  return [Number(baseAssetId || 0), baseAmount, Number(quoteAssetId || 0), quoteAmount];
}

export function humanPrice(micro, baseDecimals, quoteDecimals) {
  return (Number(micro) / 1_000_000) * 10 ** (baseDecimals - quoteDecimals);
}

export function formatDisplayAmount(value) {
  if (value == null) return EMPTY_DISPLAY;
  return Number(value).toLocaleString();
}

export function formatMicroPrice(micro, baseDecimals, quoteDecimals) {
  if (micro == null) return EMPTY_DISPLAY;
  return humanPrice(micro, baseDecimals, quoteDecimals).toLocaleString(undefined, {
    maximumFractionDigits: 8,
  });
}

export function formatAssetIdLabel(id) {
  if (id == null) return '';
  return Number(id) === 0 ? 'ALGO' : `#${id}`;
}

export function dexStatusClass(status) {
  switch (status) {
    case 'active':
    case 'active_external':
      return 'text-green-400 bg-green-500/10';
    case 'filled':
    case 'filled_external':
      return 'text-blue-400 bg-blue-500/10';
    case 'cancelled':
    case 'cancelled_external':
      return 'text-gray-400 bg-gray-500/10';
    case 'expired':
    case 'expired_external':
      return 'text-red-400 bg-red-500/10';
    default:
      return 'text-gray-400 bg-gray-500/10';
  }
}

export function dexStatusLabel(status, translate) {
  switch (status) {
    case 'active':
    case 'active_external':
      return translate('dex.statusActive');
    case 'filled':
    case 'filled_external':
      return translate('dex.statusFilled');
    case 'cancelled':
    case 'cancelled_external':
      return translate('dex.statusCancelled');
    case 'expired':
    case 'expired_external':
      return translate('dex.statusExpired');
    case 'closed_unresolved':
    case 'closed_unresolved_external':
      return translate('dex.statusClosedUnresolved');
    case 'invalid_external':
    case 'invalid_payload':
      return translate('dex.statusInvalid');
    case 'ledger_unverified':
      return translate('dex.statusLedgerUnverified');
    default:
      return status || EMPTY_DISPLAY;
  }
}

export function relativeTimeLabel(unixSec, translate, nowSec = Math.floor(Date.now() / 1000)) {
  if (!unixSec) return EMPTY_DISPLAY;
  const diff = nowSec - unixSec;
  if (diff < 60) return translate('dex.relSeconds', { count: Math.max(0, diff) });
  if (diff < 3600) return translate('dex.relMinutes', { count: Math.floor(diff / 60) });
  if (diff < 86400) return translate('dex.relHours', { count: Math.floor(diff / 3600) });
  return translate('dex.relDays', { count: Math.floor(diff / 86400) });
}

export function depthWidth(amount, maxAmount) {
  if (!maxAmount || maxAmount === 0) return 0;
  return Math.min(100, Math.round((Number(amount) / Number(maxAmount)) * 100));
}

export function pairLabelFor(pair) {
  const a = pair.asset_a === 0 ? 'ALGO' : `#${pair.asset_a}`;
  const b = pair.asset_b === 0 ? 'ALGO' : `#${pair.asset_b}`;
  return `${b}/${a}`;
}

export function pairKey(pair) {
  return `${pair.asset_a}:${pair.asset_b}`;
}
