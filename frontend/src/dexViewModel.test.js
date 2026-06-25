import test from 'node:test';
import assert from 'node:assert/strict';

import {
  assetRawBalanceFromAccount,
  cancelBatchRecoverableAssets,
  depthWidth,
  deriveOrderParams,
  humanPrice,
  roundsForSeconds,
  splitActiveOrders,
  splitProgress,
} from './lib/dex/viewModel.js';

test('derives buy and sell order params with asset decimals', () => {
  assert.deepEqual(
    deriveOrderParams({
      side: 'buy',
      amount: '1.25',
      price: '2',
      baseAssetId: 7,
      quoteAssetId: 0,
      baseDecimals: 6,
      quoteDecimals: 6,
    }),
    [0, 2_500_000, 7, 1_250_000],
  );
  assert.deepEqual(
    deriveOrderParams({
      side: 'sell',
      amount: '3',
      price: '0.5',
      baseAssetId: 7,
      quoteAssetId: 42,
      baseDecimals: 0,
      quoteDecimals: 2,
    }),
    [7, 3, 42, 150],
  );
});

test('calculates expiry rounds and orderbook display scale', () => {
  assert.equal(roundsForSeconds(10), 3);
  assert.equal(roundsForSeconds(null), 1_000_000);
  assert.equal(humanPrice(1_000_000, 6, 6), 1);
  assert.equal(humanPrice(1_000_000, 0, 6), 0.000001);
  assert.equal(depthWidth(25, 100), 25);
  assert.equal(depthWidth(200, 100), 100);
});

test('summarizes split order groups', () => {
  const orders = [
    { escrow_addr: 'b', parent_id: 'group', split_index: 1, status: 'active', sell_amount: 200, filled_amount: 50 },
    { escrow_addr: 'a', parent_id: 'group', split_index: 0, status: 'filled', sell_amount: 100, filled_amount: 100 },
    { escrow_addr: 'c', parent_id: 'group', split_index: 2, status: 'active', sell_amount: 100, filled_amount: 0 },
  ];
  assert.deepEqual(splitActiveOrders(orders, orders[0]).map((order) => order.escrow_addr), ['b', 'c']);
  assert.deepEqual(splitProgress(orders, orders[0]), { total: 400, filled: 150, pct: 38 });
});

test('reads balances and cancel recoverable assets', () => {
  const account = {
    amount: 12,
    assets: [
      { 'asset-id': 7, amount: 100 },
      { 'asset-id': 9, amount: 200 },
    ],
  };
  assert.equal(assetRawBalanceFromAccount(account, 0), 12);
  assert.equal(assetRawBalanceFromAccount(account, 7), 100);
  assert.equal(assetRawBalanceFromAccount(account, 99), 0);
  assert.deepEqual(
    cancelBatchRecoverableAssets([
      { prepare: { recoverable_asset: [9, 1] } },
      { prepare: { recoverable_asset: [7, 2] } },
      { prepare: { recoverable_asset: [9, 3] } },
    ]),
    [
      [7, 2],
      [9, 4],
    ],
  );
});
