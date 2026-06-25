import { portfolioQuery, request } from './client.js';

export const marketApi = {
  getPrice: () => request('/api/market/price'),

  getAlgoPriceHistory: (range = '1m') =>
    request(`/api/market/algo/history?${portfolioQuery(range)}`),
};
