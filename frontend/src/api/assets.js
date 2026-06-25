import { request } from './client.js';

export const assetApi = {
  prepareAssetCreate: (fields) =>
    request('/api/assets/create/prepare', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: fields.walletId,
        creator: fields.creator,
        total: fields.total,
        decimals: fields.decimals,
        unit_name: fields.unitName,
        asset_name: fields.assetName,
        url: fields.url || '',
        metadata_hash_b64: fields.metadataHashB64 || null,
        default_frozen: Boolean(fields.defaultFrozen),
        manager: fields.manager || null,
        reserve: fields.reserve || null,
        freeze: fields.freeze || null,
        clawback: fields.clawback || null,
        allow_managed_authorities: Boolean(fields.allowManagedAuthorities),
      }),
    }),

  createAsset: ({ walletId, pin, intentId, ...fields }) =>
    request('/api/assets/create', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        pin,
        intent_id: intentId,
        creator: fields.creator,
        total: fields.total,
        decimals: fields.decimals,
        unit_name: fields.unitName,
        asset_name: fields.assetName,
        url: fields.url || '',
        metadata_hash_b64: fields.metadataHashB64 || null,
        default_frozen: Boolean(fields.defaultFrozen),
        manager: fields.manager || null,
        reserve: fields.reserve || null,
        freeze: fields.freeze || null,
        clawback: fields.clawback || null,
        allow_managed_authorities: Boolean(fields.allowManagedAuthorities),
      }),
    }),

  listIssuedAssets: (walletId) =>
    request(`/api/assets/issued?wallet_id=${encodeURIComponent(walletId)}`),

  prepareAssetConfig: ({ walletId, signer, assetId, manager, reserve, freeze, clawback }) =>
    request('/api/assets/config/prepare', {
      method: 'POST',
      body: JSON.stringify({
        wallet_id: walletId,
        signer,
        asset_id: assetId,
        manager: manager || '',
        reserve: reserve || '',
        freeze: freeze || '',
        clawback: clawback || '',
      }),
    }),
};
