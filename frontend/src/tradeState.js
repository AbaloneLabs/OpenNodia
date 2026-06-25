import { get, writable } from 'svelte/store';

export const ALGO_ASSET = Object.freeze({
  id: 0,
  name: 'Algo',
  unit: 'ALGO',
  decimals: 6,
});

const EMPTY_QUOTE_STATUS = Object.freeze({
  state: 'idle',
  source: null,
  message: '',
  stale: false,
});

function initialState(network = 'local') {
  return {
    network,
    baseAsset: ALGO_ASSET,
    quoteAsset: null,
    quoteStatus: EMPTY_QUOTE_STATUS,
    resetSeq: 0,
  };
}

export const tradeState = writable(initialState());

export function normalizeAsset(asset, fallback = null) {
  if (!asset) return fallback;
  const id = Number(asset.id);
  if (!Number.isSafeInteger(id) || id < 0) return fallback;
  if (id === 0) return ALGO_ASSET;
  return {
    id,
    name: asset.name || `#${id}`,
    unit: asset.unit || `#${id}`,
    decimals: Number.isInteger(asset.decimals) ? asset.decimals : 6,
    manual: Boolean(asset.manual),
  };
}

export function setTradeNetwork(network) {
  const nextNetwork = network || 'local';
  let changed = false;
  tradeState.update((state) => {
    if (state.network === nextNetwork) return state;
    changed = true;
    return {
      ...initialState(nextNetwork),
      resetSeq: state.resetSeq + 1,
    };
  });
  return changed;
}

export function setTradeBaseAsset(asset) {
  tradeState.update((state) => ({
    ...state,
    baseAsset: normalizeAsset(asset, ALGO_ASSET),
    quoteStatus: EMPTY_QUOTE_STATUS,
  }));
}

export function setTradeQuoteAsset(asset) {
  tradeState.update((state) => ({
    ...state,
    quoteAsset: normalizeAsset(asset, null),
    quoteStatus: EMPTY_QUOTE_STATUS,
  }));
}

export function swapTradePair() {
  tradeState.update((state) => {
    if (!state.quoteAsset) return state;
    return {
      ...state,
      baseAsset: state.quoteAsset,
      quoteAsset: state.baseAsset,
      quoteStatus: EMPTY_QUOTE_STATUS,
    };
  });
}

export function resetTradeTransient() {
  tradeState.update((state) => ({
    ...state,
    quoteStatus: EMPTY_QUOTE_STATUS,
    resetSeq: state.resetSeq + 1,
  }));
}

export function setQuoteStatus(update) {
  tradeState.update((state) => ({
    ...state,
    quoteStatus: {
      ...EMPTY_QUOTE_STATUS,
      ...update,
    },
  }));
}

export function currentPairKey() {
  const state = get(tradeState);
  if (!state.quoteAsset) return '';
  const base = Number(state.baseAsset?.id ?? 0);
  const quote = Number(state.quoteAsset.id);
  return base < quote ? `${base}:${quote}` : `${quote}:${base}`;
}
