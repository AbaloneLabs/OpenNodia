import { request } from './client.js';

export const walletApi = {
  listWallets: () => request('/api/wallets'),

  createWallet: (name, pin) =>
    request('/api/wallets/create', {
      method: 'POST',
      body: JSON.stringify({ name, pin }),
    }),

  importWallet: (name, mnemonic, pin) =>
    request('/api/wallets/import', {
      method: 'POST',
      body: JSON.stringify({ name, mnemonic, pin }),
    }),

  activateWallet: (walletId) =>
    request('/api/wallets/activate', {
      method: 'POST',
      body: JSON.stringify({ wallet_id: walletId }),
    }),

  getActiveWallet: () => request('/api/wallets/active'),

  listAddresses: (walletId, pin) =>
    request(`/api/wallets/${walletId}/addresses`, {
      method: 'POST',
      body: JSON.stringify({ pin }),
    }),

  generateAddress: (walletId, pin) =>
    request(`/api/wallets/${walletId}/address`, {
      method: 'POST',
      body: JSON.stringify({ pin }),
    }),

  removeWallet: (walletId) =>
    request(`/api/wallets/${walletId}`, { method: 'DELETE' }),

  renameWallet: (walletId, name) =>
    request(`/api/wallets/${walletId}`, {
      method: 'PATCH',
      body: JSON.stringify({ name }),
    }),
};
