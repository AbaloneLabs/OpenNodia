import { writable } from 'svelte/store';

// Global active wallet state — shared across all views.
// When the user switches wallets in the header dropdown, this updates
// and every view (DEX, Assets) reacts accordingly.
export const activeWallet = writable(null);  // { id, name, source, first_address } | null
export const walletList = writable([]);      // Array of wallet objects
