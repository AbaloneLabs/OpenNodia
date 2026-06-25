import test from 'node:test';
import assert from 'node:assert/strict';
import { get } from 'svelte/store';

import {
  ALGO_ASSET,
  currentPairKey,
  normalizeAsset,
  setQuoteStatus,
  setTradeBaseAsset,
  setTradeNetwork,
  setTradeQuoteAsset,
  swapTradePair,
  tradeState,
} from './tradeState.js';

test('normalizes ALGO and ASA asset selections', () => {
  assert.equal(normalizeAsset({ id: 0 }).unit, 'ALGO');
  assert.deepEqual(normalizeAsset(null, ALGO_ASSET), ALGO_ASSET);
  assert.equal(normalizeAsset({ id: '123', decimals: 2 }).unit, '#123');
});

test('keeps a shared pair and canonical key', () => {
  setTradeNetwork('testnet');
  setTradeBaseAsset({ id: 0 });
  setTradeQuoteAsset({ id: 42, unit: 'TST', decimals: 6 });
  assert.equal(currentPairKey(), '0:42');

  swapTradePair();
  const state = get(tradeState);
  assert.equal(state.baseAsset.id, 42);
  assert.equal(state.quoteAsset.id, 0);
  assert.equal(currentPairKey(), '0:42');
});

test('network changes clear stale pair and quote state', () => {
  setTradeNetwork('testnet');
  setTradeQuoteAsset({ id: 99 });
  setQuoteStatus({ state: 'ready', source: 'Orderbook', message: 'ready' });
  const before = get(tradeState).resetSeq;

  assert.equal(setTradeNetwork('mainnet'), true);
  const state = get(tradeState);
  assert.equal(state.network, 'mainnet');
  assert.equal(state.baseAsset.id, 0);
  assert.equal(state.quoteAsset, null);
  assert.equal(state.quoteStatus.state, 'idle');
  assert.equal(state.resetSeq, before + 1);
});
