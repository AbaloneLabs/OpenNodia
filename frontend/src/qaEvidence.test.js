import test from 'node:test';
import assert from 'node:assert/strict';

import { rejectionMatrixCaseHints, routeMatrixCaseHints, routeSourcesFromCandidates } from './qaEvidence.js';

test('classifies DEX expiry boundary rejections without broad session expiry matches', () => {
  assert.deepEqual(rejectionMatrixCaseHints('order expired: current round 20 > expire round 19'), {
    dex_additional_matrix: ['expiry_boundary'],
    unified_routing: [],
  });
  assert.deepEqual(rejectionMatrixCaseHints('invalid or expired session'), {
    dex_additional_matrix: [],
    unified_routing: [],
  });
});

test('classifies duplicate submit rejections from ledger and lease wording', () => {
  assert.deepEqual(rejectionMatrixCaseHints('submit transaction group: transaction already in ledger'), {
    dex_additional_matrix: ['duplicate_submit'],
    unified_routing: [],
  });
  assert.deepEqual(rejectionMatrixCaseHints('fill lease already used by another transaction'), {
    dex_additional_matrix: ['duplicate_submit'],
    unified_routing: [],
  });
});

test('classifies DEX intent reuse separately from quote state-change rejections', () => {
  assert.deepEqual(rejectionMatrixCaseHints('DEX intent is missing, expired, or already used'), {
    dex_additional_matrix: ['intent_reuse'],
    unified_routing: [],
  });
  assert.deepEqual(rejectionMatrixCaseHints('quote_id no longer matches the current route'), {
    dex_additional_matrix: [],
    unified_routing: ['quote_then_state_change_rejected'],
  });
});

test('classifies reversed pair routing only when decimals differ', () => {
  const result = { tx_id: 'A'.repeat(52), source_type: 'orderbook' };
  const extra = {
    route_hash: 'selected',
    source_type: 'orderbook',
    route_candidates: [
      {
        route_hash: 'selected',
        source_type: 'orderbook',
        asset_in: 42,
        asset_out: 7,
        amount_out: 100,
        network_fee_microalgo: 0,
      },
    ],
  };

  assert.deepEqual(
    routeMatrixCaseHints('route', result, extra, {
      base_asset_id: 7,
      quote_asset_id: 42,
      base_decimals: 0,
      quote_decimals: 6,
    }).unified_routing,
    ['orderbook_only_pair', 'pair_reversal_decimals'],
  );
  assert.deepEqual(
    routeMatrixCaseHints('route', result, extra, {
      base_asset_id: 7,
      quote_asset_id: 42,
      base_decimals: 6,
      quote_decimals: 6,
    }).unified_routing,
    ['orderbook_only_pair'],
  );
});

test('classifies Folks-backed duplicate pool evidence only from explicit wording', () => {
  const result = { tx_id: 'B'.repeat(52), source_type: 'external_pool' };
  const extra = {
    route_hash: 'pact',
    source_type: 'external_pool',
    route_candidates: [
      {
        route_hash: 'pact',
        source_type: 'external_pool',
        source_label: 'pact',
        asset_in: 0,
        asset_out: 42,
        amount_out: 100,
        note:
          'Folks-backed Pact pool quote-only candidate; liquidity is still executed through the underlying Pact pool and is not counted as an additional AMM source',
      },
    ],
  };

  assert.deepEqual(routeMatrixCaseHints('route', result, extra).unified_routing, [
    'tinyman_pact_pool_only_pair',
    'duplicate_folks_backed_pool',
  ]);
});

test('collects split leg route sources for unified routing evidence', () => {
  assert.deepEqual(
    routeSourcesFromCandidates([
      {
        source_type: 'split',
        split_legs: [{ source_type: 'native_pool' }, { source_type: 'orderbook' }],
      },
    ]),
    ['split', 'native_pool', 'orderbook'],
  );
});

test('normalizes server route source names for unified routing evidence', () => {
  assert.deepEqual(
    routeSourcesFromCandidates([
      { source: 'native_amm', source_label: 'OpenNodia AMM' },
      { source: 'external_tinyman', source_label: 'Tinyman' },
      { source: 'external_pact', source_label: 'Pact' },
    ]),
    ['native_pool', 'external_pool', 'tinyman', 'pact'],
  );
});

test('classifies normalized native and external route-only evidence', () => {
  assert.deepEqual(
    routeMatrixCaseHints(
      'route',
      { tx_id: 'C'.repeat(52), source_type: 'native_amm' },
      {
        route_hash: 'native',
        source_type: 'native_amm',
        route_candidates: [
          {
            route_hash: 'native',
            source: 'native_amm',
            source_label: 'OpenNodia AMM',
            asset_in: 0,
            asset_out: 42,
            amount_out: 100,
          },
        ],
      },
    ).unified_routing,
    ['native_pool_only_pair'],
  );
  assert.deepEqual(
    routeMatrixCaseHints(
      'route',
      { tx_id: 'D'.repeat(52), source_type: 'external_tinyman' },
      {
        route_hash: 'tinyman',
        source_type: 'external_tinyman',
        route_candidates: [
          {
            route_hash: 'tinyman',
            source: 'external_tinyman',
            source_label: 'Tinyman',
            asset_in: 0,
            asset_out: 42,
            amount_out: 100,
          },
        ],
      },
    ).unified_routing,
    ['tinyman_pact_pool_only_pair'],
  );
});
