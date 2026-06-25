import test from 'node:test';
import assert from 'node:assert/strict';

import {
  assetMetaFromCache,
  externalPoolCanMutateLiquidity,
  formatBps,
  formatSharePpm,
  normalizeAssetId,
  poolStateDiffers,
  rawAmountFromDecimal,
  recommendedFeeForProfile,
  scaledRateToBps,
  sourceCapability,
} from './lib/lpTrade/viewModel.js';

test('normalizes LP asset ids and formatting helpers', () => {
  assert.equal(normalizeAssetId('0'), 0);
  assert.equal(normalizeAssetId('123'), 123);
  assert.equal(normalizeAssetId(''), null);
  assert.equal(normalizeAssetId('-1'), null);
  assert.equal(formatSharePpm(12345), '1.23%');
  assert.equal(formatBps(5), '0.05%');
  assert.equal(scaledRateToBps(10000000000000000n), 10000);
});

test('resolves asset metadata and parses raw decimal amounts', () => {
  const cache = new Map([[7, { id: 7, unit: 'USD', decimals: 2 }]]);
  assert.equal(assetMetaFromCache(0, cache).unit, 'ALGO');
  assert.deepEqual(assetMetaFromCache(7, cache), { id: 7, unit: 'USD', decimals: 2 });
  assert.equal(rawAmountFromDecimal('12.34', 7, (assetId) => assetMetaFromCache(assetId, cache)), 1234);
});

test('classifies external source capabilities and liquidity writes', () => {
  const externalStatus = {
    sources: [
      { source: 'tinyman', quote_supported: true, swap_supported: true, liquidity_supported: true },
      { source: 'pact', quote_supported: true, swap_supported: false, liquidity_supported: false },
    ],
  };
  assert.equal(sourceCapability(externalStatus, 'tinyman'), 'ready');
  assert.equal(sourceCapability(externalStatus, 'pact'), 'quoteOnly');
  assert.equal(sourceCapability(externalStatus, 'missing'), 'offline');
  assert.equal(
    externalPoolCanMutateLiquidity(
      {
        source: 'tinyman',
        tradable: true,
        adapter_swap_supported: true,
        folks_backed: false,
        lp_asset_id: 42,
      },
      externalStatus,
    ),
    true,
  );
  assert.equal(
    externalPoolCanMutateLiquidity(
      {
        source: 'pact',
        tradable: true,
        adapter_swap_supported: true,
        folks_backed: false,
        lp_asset_id: 42,
      },
      externalStatus,
    ),
    false,
  );
});

test('detects stale pools and profile fee defaults', () => {
  const pool = { source_round: 1, reserve_0: 2, reserve_1: 3, total_lp_supply: 4 };
  assert.equal(poolStateDiffers(pool, { ...pool }), false);
  assert.equal(poolStateDiffers(pool, { ...pool, reserve_1: 5 }), true);
  assert.equal(recommendedFeeForProfile('verifiedPeg'), 5);
  assert.equal(recommendedFeeForProfile('volatile'), 100);
  assert.equal(recommendedFeeForProfile('standard'), 30);
});
