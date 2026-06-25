import assert from 'node:assert/strict';
import test from 'node:test';

import { orderLinkUiState } from './orderLinkState.js';

test('order link errors and tampered payloads cannot be acted on', () => {
  assert.deepEqual(orderLinkUiState(null, 'invalid order link'), {
    kind: 'error',
    labelKey: 'dex.orderLinkInvalid',
    canAct: false,
  });

  assert.deepEqual(
    orderLinkUiState({
      payload_valid: false,
      canonical_escrow_match: false,
      verification: null,
    }),
    {
      kind: 'invalid',
      labelKey: 'dex.orderLinkTampered',
      canAct: false,
    },
  );
});

test('empty and expired order links stay visible but not executable', () => {
  assert.deepEqual(
    orderLinkUiState({
      payload_valid: true,
      canonical_escrow_match: true,
      status: 'invalid_external',
      verification: { valid: false, expired: false },
    }),
    {
      kind: 'warning',
      labelKey: 'dex.orderLinkLedgerInactive',
      canAct: false,
    },
  );

  assert.deepEqual(
    orderLinkUiState({
      payload_valid: true,
      canonical_escrow_match: true,
      status: 'expired_external',
      verification: { valid: false, expired: true },
    }),
    {
      kind: 'expired',
      labelKey: 'dex.orderLinkExpired',
      canAct: false,
    },
  );
});

test('verified active order links can be acted on', () => {
  assert.deepEqual(
    orderLinkUiState({
      payload_valid: true,
      canonical_escrow_match: true,
      status: 'active_external',
      verification: { valid: true, expired: false },
    }),
    {
      kind: 'verified',
      labelKey: 'dex.orderLinkVerified',
      canAct: true,
    },
  );
});
