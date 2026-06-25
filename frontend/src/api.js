import { accountApi } from './api/accounts.js';
import { analyticsApi } from './api/analytics.js';
import { assetApi } from './api/assets.js';
import { authApi } from './api/auth.js';
import { connectEvents, getToken, setToken } from './api/client.js';
import { dexApi } from './api/dex.js';
import { lpApi } from './api/lp.js';
import { marketApi } from './api/market.js';
import { transferApi } from './api/transfers.js';
import { walletApi } from './api/wallets.js';

export { getToken, setToken };

export const api = {
  ...accountApi,
  ...analyticsApi,
  ...assetApi,
  ...authApi,
  ...dexApi,
  ...lpApi,
  ...marketApi,
  ...transferApi,
  ...walletApi,
  connectEvents,
};
