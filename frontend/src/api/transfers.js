import { request } from './client.js';

export const transferApi = {
  prepareTransfer: ({ from, to, assetId, amount, note }) =>
    request('/api/transfer/prepare', {
      method: 'POST',
      body: JSON.stringify({
        from,
        to,
        asset_id: assetId,
        amount,
        note: note || null,
      }),
    }),

  sendTransfer: ({ walletId, pin, from, to, assetId, amount, note }) =>
    request('/api/transfer/send', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        from,
        to,
        asset_id: assetId,
        amount,
        note: note || null,
      }),
    }),

  prepareOptIn: ({ address, assetId }) =>
    request('/api/transfer/opt-in/prepare', {
      method: 'POST',
      body: JSON.stringify({
        address,
        asset_id: assetId,
      }),
    }),

  optInAsset: ({ walletId, pin, address, assetId }) =>
    request('/api/transfer/opt-in', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        address,
        asset_id: assetId,
      }),
    }),
};
