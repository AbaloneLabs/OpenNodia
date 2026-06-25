import test from 'node:test';
import assert from 'node:assert/strict';

import {
  formatRawAmount,
  multiplyDecimalsToRaw,
  parseDecimalToRaw,
  rawToSafeNumber,
  utf8ByteLength,
} from './amount.js';

test('converts decimal strings without floating point math', () => {
  assert.equal(parseDecimalToRaw('1.000001', 6), 1_000_001n);
  assert.equal(parseDecimalToRaw('42', 0), 42n);
  assert.equal(parseDecimalToRaw('0.1', 6), 100_000n);
});

test('rejects invalid precision and zero amounts', () => {
  assert.throws(() => parseDecimalToRaw('1.0000001', 6));
  assert.throws(() => parseDecimalToRaw('0', 6));
  assert.throws(() => parseDecimalToRaw('1e3', 6));
});

test('formats raw amounts and checks safe integer range', () => {
  assert.equal(formatRawAmount(1_230_000n, 6), '1.23');
  assert.equal(formatRawAmount(42n, 0), '42');
  assert.equal(rawToSafeNumber(123n), 123);
  assert.throws(() => rawToSafeNumber(BigInt(Number.MAX_SAFE_INTEGER) + 1n));
});

test('counts UTF-8 bytes', () => {
  assert.equal(utf8ByteLength('abc'), 3);
  assert.equal(utf8ByteLength('한'), 3);
});

test('multiplies decimal amount and price without floating point math', () => {
  assert.equal(multiplyDecimalsToRaw('1.25', '2.4', 6), 3_000_000n);
  assert.throws(() => multiplyDecimalsToRaw('0.1', '0.0000001', 6));
});
