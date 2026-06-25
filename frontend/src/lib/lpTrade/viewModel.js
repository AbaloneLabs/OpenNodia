import { formatRawAmount, parseDecimalToRaw, rawToSafeNumber } from '../../amount.js';

export const ALGO_META = Object.freeze({ id: 0, name: 'Algo', unit: 'ALGO', decimals: 6 });

export function fmt(value) {
  return String(value ?? '0');
}

export function normalizeAssetId(value) {
  const text = String(value ?? '').trim();
  if (!text) return null;
  const id = Number(text);
  return Number.isSafeInteger(id) && id >= 0 ? id : null;
}

export function assetMetaFromCache(assetId, cache) {
  const id = Number(assetId);
  if (id === 0) return ALGO_META;
  return cache?.get(id) || { id, name: `#${id}`, unit: `#${id}`, decimals: 6 };
}

export function assetLabelFromMeta(meta) {
  return meta.unit ? `${meta.unit} (${meta.id})` : `#${meta.id}`;
}

export function formatAssetWithMeta(raw, meta) {
  try {
    return `${formatRawAmount(BigInt(raw ?? 0), meta.decimals)} ${assetLabelFromMeta(meta)}`;
  } catch (_) {
    return `${fmt(raw)} ${assetLabelFromMeta(meta)}`;
  }
}

export function formatLp(raw) {
  return formatRawAmount(BigInt(raw ?? 0), 6);
}

export function formatSharePpm(ppm) {
  const value = Number(ppm || 0) / 10_000;
  return `${value.toFixed(value > 0 && value < 0.01 ? 4 : 2)}%`;
}

export function formatBps(bps) {
  const value = Number(bps || 0) / 100;
  return `${value.toFixed(value > 0 && value < 0.01 ? 4 : 2)}%`;
}

export function scaledRateToBps(rate) {
  try {
    return Number((BigInt(rate ?? 0) * 10000n) / 10000000000000000n);
  } catch (_) {
    return 0;
  }
}

export function sourceStatus(externalStatus, source) {
  return externalStatus?.sources?.find((item) => item.source === source) || null;
}

export function sourceCapability(externalStatus, source) {
  const item = sourceStatus(externalStatus, source);
  if (!item?.quote_supported) return 'offline';
  return item.swap_supported ? 'ready' : 'quoteOnly';
}

export function sourceBadgeClass(externalStatus, source) {
  const item = sourceStatus(externalStatus, source);
  if (item?.quote_supported) return 'bg-algo-500/10 text-algo-300';
  return 'bg-gray-700/60 text-gray-400';
}

export function externalPoolCanMutateLiquidity(poolInfo, externalStatus) {
  if (!poolInfo || poolInfo.source === 'native') return false;
  const item = sourceStatus(externalStatus, poolInfo.source);
  return Boolean(
    item?.liquidity_supported &&
      poolInfo.tradable &&
      poolInfo.adapter_swap_supported &&
      !poolInfo.folks_backed &&
      poolInfo.lp_asset_id,
  );
}

export function rawAmountFromDecimal(value, assetId, resolveAssetMeta) {
  return rawToSafeNumber(parseDecimalToRaw(value, resolveAssetMeta(assetId).decimals));
}

export function poolStateDiffers(left, right) {
  return (
    !left ||
    !right ||
    Number(left.source_round) !== Number(right.source_round) ||
    Number(left.reserve_0) !== Number(right.reserve_0) ||
    Number(left.reserve_1) !== Number(right.reserve_1) ||
    Number(left.total_lp_supply) !== Number(right.total_lp_supply)
  );
}

export function recommendedFeeForProfile(profile) {
  if (profile === 'verifiedPeg') return 5;
  if (profile === 'volatile') return 100;
  return 30;
}
