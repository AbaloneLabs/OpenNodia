export const ASSET_COLOR_LABELS = [
  '',
  'slate',
  'red',
  'orange',
  'yellow',
  'green',
  'cyan',
  'blue',
  'purple',
  'pink',
];

export function metadataArrayToMap(records = []) {
  const map = new Map();
  for (const record of records || []) {
    map.set(Number(record.asset_id), normalizeAssetMetadata(record));
  }
  return map;
}

export function normalizeAssetMetadata(record = {}) {
  return {
    asset_id: Number(record.asset_id ?? 0),
    tag: String(record.tag || ''),
    memo: String(record.memo || ''),
    color_label: ASSET_COLOR_LABELS.includes(record.color_label) ? record.color_label : '',
    pinned: Boolean(record.pinned),
    updated_at: Number(record.updated_at || 0),
  };
}

export function emptyAssetMetadata(assetId) {
  return {
    asset_id: Number(assetId),
    tag: '',
    memo: '',
    color_label: '',
    pinned: false,
    updated_at: 0,
  };
}

export function getAssetMetadata(metadataMap, assetId) {
  return metadataMap?.get(Number(assetId)) || emptyAssetMetadata(assetId);
}

export function setAssetMetadataInMap(metadataMap, record) {
  const next = new Map(metadataMap || []);
  const normalized = normalizeAssetMetadata(record);
  if (!hasAssetMetadataContent(normalized)) {
    next.delete(normalized.asset_id);
  } else {
    next.set(normalized.asset_id, normalized);
  }
  return next;
}

export function hasAssetMetadataContent(record) {
  return Boolean(record?.pinned || record?.tag || record?.memo || record?.color_label);
}

export function availableTags(metadataMap) {
  return [...new Set([...metadataMap.values()].map((record) => record.tag).filter(Boolean))].sort((a, b) =>
    a.localeCompare(b),
  );
}

export function computeAssetDisplay(data, metadataMap, sort = 'balance-desc', filters = {}) {
  if (!data || !data.assets) return { algo: null, pinned: [], rest: [] };
  const algo = data.assets.find((asset) => asset.kind === 'native') || null;
  const asas = data.assets.filter((asset) => asset.kind === 'asa');
  const tagFilter = filters.tag || 'all';
  const policyFilter = filters.policy || 'all';
  const balanceFilter = filters.balance || 'all';

  const pinned = [];
  const rest = [];

  for (const asset of asas) {
    const metadata = getAssetMetadata(metadataMap, asset.id);
    if (tagFilter !== 'all' && metadata.tag !== tagFilter) continue;
    if (policyFilter !== 'all' && asset.policy !== policyFilter) continue;
    if (balanceFilter === 'nonzero' && asset.amount === 0) continue;
    if (balanceFilter === 'zero' && asset.amount !== 0) continue;

    if (metadata.pinned) {
      pinned.push(asset);
    } else {
      rest.push(asset);
    }
  }

  const sortFn = (arr) => {
    const sorted = [...arr];
    switch (sort) {
      case 'balance-desc':
        sorted.sort((a, b) => b.amount - a.amount);
        break;
      case 'balance-asc':
        sorted.sort((a, b) => a.amount - b.amount);
        break;
      case 'name':
        sorted.sort((a, b) => (a.name || '').localeCompare(b.name || ''));
        break;
    }
    return sorted;
  };

  return {
    algo,
    pinned: sortFn(pinned),
    rest: sortFn(rest),
  };
}
