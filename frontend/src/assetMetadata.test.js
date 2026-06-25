import assert from 'node:assert/strict';
import test from 'node:test';

import {
  availableTags,
  computeAssetDisplay,
  metadataArrayToMap,
  setAssetMetadataInMap,
} from './assetMetadata.js';

test('normalizes metadata records into an asset keyed map', () => {
  const map = metadataArrayToMap([
    { asset_id: '42', tag: 'Stable', memo: 'core', color_label: 'blue', pinned: true },
    { asset_id: 7, tag: 'LP', color_label: 'invalid', pinned: false },
  ]);
  assert.equal(map.get(42).pinned, true);
  assert.equal(map.get(7).color_label, '');
  assert.deepEqual(availableTags(map), ['LP', 'Stable']);
});

test('filters and tiers assets by local metadata without changing ALGO', () => {
  const assets = {
    assets: [
      { kind: 'native', id: 0, name: 'Algo', amount: 10 },
      { kind: 'asa', id: 1, name: 'USD', amount: 0, policy: 'open' },
      { kind: 'asa', id: 2, name: 'Bond', amount: 50, policy: 'regulated' },
      { kind: 'asa', id: 3, name: 'LP', amount: 5, policy: 'open' },
    ],
  };
  const metadata = metadataArrayToMap([
    { asset_id: 1, tag: 'Stable', pinned: true },
    { asset_id: 3, tag: 'LP', pinned: false },
  ]);

  const display = computeAssetDisplay(assets, metadata, 'name', {
    tag: 'Stable',
    policy: 'open',
    balance: 'all',
  });
  assert.equal(display.algo.id, 0);
  assert.deepEqual(display.pinned.map((asset) => asset.id), [1]);
  assert.deepEqual(display.rest, []);

  const nonzero = computeAssetDisplay(assets, metadata, 'balance-desc', {
    tag: 'all',
    policy: 'open',
    balance: 'nonzero',
  });
  assert.deepEqual(nonzero.rest.map((asset) => asset.id), [3]);
});

test('removes empty unpinned metadata from a map', () => {
  const map = metadataArrayToMap([{ asset_id: 42, tag: 'Stable', pinned: true }]);
  const next = setAssetMetadataInMap(map, { asset_id: 42, tag: '', memo: '', color_label: '', pinned: false });
  assert.equal(next.has(42), false);
});
