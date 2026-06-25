import { analyticsQuery, portfolioQuery, request, requestText, transactionQuery } from './client.js';

export const analyticsApi = {
  searchAssets: (query) =>
    request(`/api/assets/search?q=${encodeURIComponent(query)}`),

  getAssetMetadata: (assetId) =>
    request(`/api/assets/${assetId}`),

  getAsset: (assetId) =>
    request(`/api/assets/${assetId}`),

  getAccountTransactions: (address, params = {}) =>
    request(`/api/accounts/${encodeURIComponent(address)}/transactions?${transactionQuery(params)}`),

  exportAccountTransactionsCsv: (address, params = {}) =>
    requestText(`/api/accounts/${encodeURIComponent(address)}/transactions.csv?${transactionQuery(params)}`),

  getBalanceSnapshots: (address, months = 12) =>
    request(`/api/accounts/${encodeURIComponent(address)}/balance-snapshots?months=${months}`),

  getPortfolioValue: (address, range = '1m') =>
    request(`/api/accounts/${encodeURIComponent(address)}/portfolio?${portfolioQuery(range)}`),

  getPortfolioHistory: (address, range = '1m') =>
    request(`/api/accounts/${encodeURIComponent(address)}/portfolio-history?${portfolioQuery(range)}`),

  getWalletPortfolioValues: (range = '1d') =>
    request(`/api/wallets/portfolio-values?${portfolioQuery(range)}`),

  getCreatorAssets: (creator, params = {}) =>
    request(`/api/analytics/assets/creator/${encodeURIComponent(creator)}?${analyticsQuery(params)}`),

  getAssetHolders: (assetId, params = {}) =>
    request(`/api/analytics/assets/${encodeURIComponent(assetId)}/holders?${analyticsQuery(params)}`),

  getAssetApplications: (assetId, params = {}) =>
    request(`/api/analytics/assets/${encodeURIComponent(assetId)}/applications?${analyticsQuery(params)}`),

  getAssetTransactionsAnalysis: (assetId, params = {}) =>
    request(`/api/analytics/assets/${encodeURIComponent(assetId)}/transactions?${analyticsQuery(params)}`),

  getIndexerStatus: () => request('/api/indexer/status'),

  getIndexerSyncProgress: () => request('/api/indexer/sync-progress'),
};
