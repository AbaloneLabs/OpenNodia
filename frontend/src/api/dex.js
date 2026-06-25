import { request } from './client.js';

export const dexApi = {
  prepareCreate: (walletId, signer, side, sellAssetId, sellAmount, buyAssetId, buyAmount, expireRounds = 10000, splitCount = 1) =>
    request('/api/dex/prepare/create', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        signer,
        side,
        sell_asset_id: sellAssetId,
        sell_amount: sellAmount,
        buy_asset_id: buyAssetId,
        buy_amount: buyAmount,
        expire_rounds: expireRounds,
        split_count: splitCount,
      }),
    }),

  submitCreate: (walletId, pin, intentId) =>
    request('/api/dex/submit/create', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
      }),
    }),

  prepareFill: (walletId, filler, escrowAddress) =>
    request('/api/dex/prepare/fill', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        filler,
        escrow_address: escrowAddress,
      }),
    }),

  submitFill: (walletId, pin, intentId) =>
    request('/api/dex/submit/fill', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
      }),
    }),

  prepareCancel: (walletId, escrowAddress) =>
    request('/api/dex/prepare/cancel', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        escrow_address: escrowAddress,
      }),
    }),

  submitCancel: (walletId, pin, intentId) =>
    request('/api/dex/submit/cancel', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
      }),
    }),

  prepareRoute: (
    walletId,
    filler,
    side,
    sellAssetId,
    sellAmount,
    buyAssetId,
    buyAmount,
    splitCount = 1,
    immediateFill = true,
    expireRounds = 10000,
    placeRemaining = false,
  ) =>
    request('/api/dex/prepare/route', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        filler,
        side,
        sell_asset_id: sellAssetId,
        sell_amount: sellAmount,
        buy_asset_id: buyAssetId,
        buy_amount: buyAmount,
        split_count: splitCount,
        immediate_fill: immediateFill,
        place_remaining: placeRemaining,
        expire_rounds: expireRounds,
      }),
    }),

  submitRoute: (walletId, pin, intentId) =>
    request('/api/dex/submit/route', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
      }),
    }),

  getDexStatus: () => request('/api/dex/status'),

  getPairs: (limit = 12) =>
    request(`/api/dex/pairs?limit=${limit}`),

  getOrderbook: (assetA, assetB, depth = 20) =>
    request(`/api/dex/orderbook?asset_a=${assetA}&asset_b=${assetB}&depth=${depth}`),

  listCommunityMarkets: ({ operator, assetId, q, limit = 12 } = {}) => {
    const qs = new URLSearchParams();
    if (operator) qs.set('operator', operator);
    if (assetId !== '' && assetId != null) qs.set('asset_id', assetId);
    if (q) qs.set('q', q);
    qs.set('limit', limit);
    return request(`/api/dex/markets?${qs.toString()}`);
  },

  getCommunityMarket: (id) =>
    request(`/api/dex/markets/${encodeURIComponent(id)}`),

  createCommunityMarket: (market) =>
    request('/api/dex/markets', {
      method: 'POST',
      body: JSON.stringify(market),
    }),

  updateCommunityMarket: (id, market) =>
    request(`/api/dex/markets/${encodeURIComponent(id)}`, {
      method: 'PUT',
      body: JSON.stringify(market),
    }),

  getCommunityMarketPairs: (id) =>
    request(`/api/dex/markets/${encodeURIComponent(id)}/pairs`),

  getCommunityMarketOrderbook: (id, assetA, assetB, depth = 20) =>
    request(
      `/api/dex/markets/${encodeURIComponent(id)}/orderbook?asset_a=${assetA}&asset_b=${assetB}&depth=${depth}`,
    ),

  getCommunityMarketTrades: (id, assetA, assetB, limit = 30) =>
    request(
      `/api/dex/markets/${encodeURIComponent(id)}/trades?asset_a=${assetA}&asset_b=${assetB}&limit=${limit}`,
    ),

  getRouteCandidates: ({ assetIn, assetOut, amountIn, slippageBps, depth }) =>
    request('/api/dex/routes', {
      method: 'POST',
      body: JSON.stringify({
        asset_in: String(assetIn),
        asset_out: String(assetOut),
        amount_in: String(amountIn),
        slippage_bps: Number(slippageBps || 50),
        depth: Number(depth || 20),
      }),
    }),

  getRouterQuote: ({ assetIn, assetOut, amountIn, slippageBps, depth, source }) =>
    request('/api/router/quote', {
      method: 'POST',
      body: JSON.stringify({
        asset_in: String(assetIn),
        asset_out: String(assetOut),
        amount_in: String(amountIn),
        slippage_bps: Number(slippageBps || 50),
        depth: Number(depth || 20),
        source: source || 'best',
      }),
    }),

  prepareRouter: ({ walletId, trader, quote, routeHash, expireRounds }) =>
    request('/api/router/prepare', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        trader,
        quote_id: quote.quote_id,
        route_hash: routeHash,
        asset_in: String(quote.asset_in),
        asset_out: String(quote.asset_out),
        amount_in: String(quote.amount_in),
        slippage_bps: Number(quote.slippage_bps || 50),
        depth: Number(quote.depth || 20),
        source: quote.source || 'best',
        expire_rounds: Number(expireRounds || 1000),
      }),
    }),

  submitRouter: ({ walletId, pin, intentId, quoteId, routeHash }) =>
    request('/api/router/submit', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
        quote_id: quoteId,
        route_hash: routeHash,
      }),
    }),

  getMyOrders: (walletId, status = 'all') =>
    request(`/api/dex/orders?wallet_id=${encodeURIComponent(walletId)}&status=${status}`),

  getTrades: (params) => {
    const qs = new URLSearchParams();
    if (params.pair) qs.set('pair', params.pair);
    if (params.address) qs.set('address', params.address);
    qs.set('limit', params.limit || 50);
    return request(`/api/dex/trades?${qs.toString()}`);
  },

  getOrderDetail: (escrowAddress) =>
    request(`/api/dex/order/${escrowAddress}`),

  getOrderLink: (escrowAddress) =>
    request(`/api/dex/order/${escrowAddress}/link`),

  getOrderLinkDetail: (payload) =>
    request(`/api/dex/order-link/${encodeURIComponent(payload)}`),
};
