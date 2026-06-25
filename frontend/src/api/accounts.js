import { request } from './client.js';

export const accountApi = {
  getNodeStatus: () => request('/api/node/status'),

  getSyncProgress: () => request('/api/node/sync-progress'),

  getBlockInfo: () => request('/api/node/block-info'),

  getParticipationStats: () => request('/api/node/participation-stats'),

  getAccount: (address) => request(`/api/accounts/${address}`),

  getAccountAssets: (address) => request(`/api/accounts/${address}/assets`),

  listAssetMetadata: (address) =>
    request(`/api/accounts/${encodeURIComponent(address)}/asset-metadata`),

  saveAssetMetadata: (address, assetId, metadata) =>
    request(`/api/accounts/${encodeURIComponent(address)}/assets/${encodeURIComponent(assetId)}/metadata`, {
      method: 'PUT',
      body: JSON.stringify({
        tag: metadata.tag || '',
        memo: metadata.memo || '',
        color_label: metadata.color_label || '',
        pinned: Boolean(metadata.pinned),
      }),
    }),

  clearAssetMetadata: (address, assetId) =>
    request(`/api/accounts/${encodeURIComponent(address)}/assets/${encodeURIComponent(assetId)}/metadata`, {
      method: 'DELETE',
    }),
};
