import { writable } from 'svelte/store';

// Active navigation view: 'node' | 'wallets' | 'assets' | 'dex' | 'lp' | 'asa' | 'settings'
export const activeView = writable('node');

// When navigating from Wallets to Assets, this holds the wallet ID to display.
export const selectedWalletId = writable(null);

// Decoded by AppShell from /#/dex/order/{payload}; consumed by DEXView.
export const pendingOrderLinkPayload = writable(null);

export function parseOrderLinkHash(hash) {
  const prefix = '#/dex/order/';
  if (!hash || !hash.startsWith(prefix)) return null;
  let payload = hash.slice(prefix.length);
  try {
    payload = decodeURIComponent(payload);
  } catch {
    // Keep the raw fragment; the server-side decoder will return a precise error.
  }
  return payload || null;
}
