import { request } from './client.js';

export const lpApi = {
  getLpStatus: () => request('/api/lp/status'),

  getExternalLiquidityStatus: () => request('/api/lp/external/status'),

  getLpPools: ({ assetA, assetB }) => {
    const qs = new URLSearchParams();
    if (assetA !== '' && assetA != null) qs.set('asset_a', assetA);
    if (assetB !== '' && assetB != null) qs.set('asset_b', assetB);
    return request(`/api/lp/pools?${qs.toString()}`);
  },

  getExternalLpPools: ({ assetA, assetB, source }) => {
    const qs = new URLSearchParams();
    if (assetA !== '' && assetA != null) qs.set('asset_a', assetA);
    if (assetB !== '' && assetB != null) qs.set('asset_b', assetB);
    if (source) qs.set('source', source);
    return request(`/api/lp/external/pools?${qs.toString()}`);
  },

  getExternalLpPositions: ({ address, assetA, assetB, source }) => {
    const qs = new URLSearchParams();
    qs.set('address', address);
    if (assetA !== '' && assetA != null) qs.set('asset_a', assetA);
    if (assetB !== '' && assetB != null) qs.set('asset_b', assetB);
    if (source) qs.set('source', source);
    return request(`/api/lp/external/positions?${qs.toString()}`);
  },

  getLpPositions: ({ address }) => {
    const qs = new URLSearchParams();
    qs.set('address', address);
    return request(`/api/lp/positions?${qs.toString()}`);
  },

  getLpPool: (appId) =>
    request(`/api/lp/pools/${encodeURIComponent(appId)}`),

  quoteLpSwap: ({ appId, assetIn, amountIn, slippageBps }) =>
    request('/api/lp/quote', {
      method: 'POST',
      body: JSON.stringify({
        app_id: String(appId),
        asset_in: String(assetIn),
        amount_in: String(amountIn),
        slippage_bps: Number(slippageBps || 50),
      }),
    }),

  quoteExternalLiquidity: ({ source, poolId, assetIn, amountIn, slippageBps }) =>
    request('/api/lp/external/quote', {
      method: 'POST',
      body: JSON.stringify({
        source,
        pool_id: String(poolId),
        asset_in: String(assetIn),
        amount_in: String(amountIn),
        slippage_bps: Number(slippageBps || 50),
      }),
    }),

  prepareExternalSwap: ({ walletId, source, poolId, trader, assetIn, amountIn, slippageBps, expireRounds }) =>
    request('/api/lp/external/swap/prepare', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        source,
        pool_id: String(poolId),
        trader,
        asset_in: String(assetIn),
        amount_in: String(amountIn),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  submitExternalSwap: ({ walletId, pin, intentId, source, poolId, trader, assetIn, amountIn, slippageBps, expireRounds }) =>
    request('/api/lp/external/swap', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
        source,
        pool_id: String(poolId),
        trader,
        asset_in: String(assetIn),
        amount_in: String(amountIn),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  prepareExternalAddLiquidity: ({ walletId, source, poolId, provider, amount0, amount1, slippageBps, expireRounds }) =>
    request('/api/lp/external/add/prepare', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        source,
        pool_id: String(poolId),
        provider,
        amount_0: String(amount0),
        amount_1: String(amount1),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  submitExternalAddLiquidity: ({ walletId, pin, intentId, source, poolId, provider, amount0, amount1, slippageBps, expireRounds }) =>
    request('/api/lp/external/add', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
        source,
        pool_id: String(poolId),
        provider,
        amount_0: String(amount0),
        amount_1: String(amount1),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  prepareExternalRemoveLiquidity: ({ walletId, source, poolId, provider, burnLp, slippageBps, expireRounds }) =>
    request('/api/lp/external/remove/prepare', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        source,
        pool_id: String(poolId),
        provider,
        burn_lp: String(burnLp),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  submitExternalRemoveLiquidity: ({ walletId, pin, intentId, source, poolId, provider, burnLp, slippageBps, expireRounds }) =>
    request('/api/lp/external/remove', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
        source,
        pool_id: String(poolId),
        provider,
        burn_lp: String(burnLp),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  prepareLpPoolCreate: ({ walletId, creator, assetA, assetB, feeBps }) =>
    request('/api/lp/pools/create/prepare', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        creator,
        asset_a: String(assetA),
        asset_b: String(assetB),
        fee_bps: Number(feeBps),
      }),
    }),

  createLpPool: ({ walletId, pin, intentId, creator, assetA, assetB, feeBps }) =>
    request('/api/lp/pools/create', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
        creator,
        asset_a: String(assetA),
        asset_b: String(assetB),
        fee_bps: Number(feeBps),
      }),
    }),

  prepareLpPoolSetup: ({ walletId, creator, appId, fundingMicroalgo }) =>
    request('/api/lp/pools/setup/prepare', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        creator,
        app_id: String(appId),
        funding_microalgo: Number(fundingMicroalgo || 500000),
      }),
    }),

  setupLpPool: ({ walletId, pin, intentId, creator, appId, fundingMicroalgo }) =>
    request('/api/lp/pools/setup', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
        creator,
        app_id: String(appId),
        funding_microalgo: Number(fundingMicroalgo || 500000),
      }),
    }),

  prepareLpPoolBootstrap: ({ walletId, provider, appId, amount0, amount1, slippageBps, expireRounds }) =>
    request('/api/lp/pools/bootstrap/prepare', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        provider,
        app_id: String(appId),
        amount_0: String(amount0),
        amount_1: String(amount1),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  bootstrapLpPool: ({ walletId, pin, intentId, provider, appId, amount0, amount1, slippageBps, expireRounds }) =>
    request('/api/lp/pools/bootstrap', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
        provider,
        app_id: String(appId),
        amount_0: String(amount0),
        amount_1: String(amount1),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  prepareLpPoolAdd: ({ walletId, provider, appId, desired0, desired1, slippageBps, expireRounds }) =>
    request('/api/lp/pools/add/prepare', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        provider,
        app_id: String(appId),
        desired_0: String(desired0),
        desired_1: String(desired1),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  addLpPoolLiquidity: ({ walletId, pin, intentId, provider, appId, desired0, desired1, slippageBps, expireRounds }) =>
    request('/api/lp/pools/add', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
        provider,
        app_id: String(appId),
        desired_0: String(desired0),
        desired_1: String(desired1),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  prepareLpPoolRemove: ({ walletId, provider, appId, burnLp, slippageBps, expireRounds }) =>
    request('/api/lp/pools/remove/prepare', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        provider,
        app_id: String(appId),
        burn_lp: String(burnLp),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  removeLpPoolLiquidity: ({ walletId, pin, intentId, provider, appId, burnLp, slippageBps, expireRounds }) =>
    request('/api/lp/pools/remove', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
        provider,
        app_id: String(appId),
        burn_lp: String(burnLp),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  prepareLpSwap: ({ walletId, trader, appId, assetIn, amountIn, slippageBps, expireRounds }) =>
    request('/api/lp/swap/prepare', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        trader,
        app_id: String(appId),
        asset_in: String(assetIn),
        amount_in: String(amountIn),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  swapLpExactIn: ({ walletId, pin, intentId, trader, appId, assetIn, amountIn, slippageBps, expireRounds }) =>
    request('/api/lp/swap', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
        trader,
        app_id: String(appId),
        asset_in: String(assetIn),
        amount_in: String(amountIn),
        slippage_bps: Number(slippageBps || 50),
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),
};
