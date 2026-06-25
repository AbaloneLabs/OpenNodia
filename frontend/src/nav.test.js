import assert from 'node:assert/strict';
import test from 'node:test';

import { parseOrderLinkHash } from './nav.js';

test('parses DEX order link hash payloads', () => {
  assert.equal(parseOrderLinkHash('#/dex/order/abc-123_DEF'), 'abc-123_DEF');
  assert.equal(parseOrderLinkHash('#/dex/order/abc%2B123'), 'abc+123');
});

test('ignores unrelated hashes and preserves malformed payloads', () => {
  assert.equal(parseOrderLinkHash('#/wallets'), null);
  assert.equal(parseOrderLinkHash(''), null);
  assert.equal(parseOrderLinkHash('#/dex/order/%E0%A4%A'), '%E0%A4%A');
});
