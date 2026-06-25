<script>
  import { onMount } from 'svelte';
  import { api } from '../api.js';
  import { t } from '../i18n/index.js';
  import { activeWallet, walletList } from '../walletStore.js';
  import { activeView, selectedWalletId } from '../nav.js';

  let open = false;
  let loading = false;

  async function loadWallets() {
    loading = true;
    try {
      const wallets = await api.listWallets();
      walletList.set(wallets);
      const active = await api.getActiveWallet();
      activeWallet.set(active.wallet);
    } catch (e) {
      // ignore
    } finally {
      loading = false;
    }
  }

  async function switchWallet(w) {
    try {
      await api.activateWallet(w.id);
      // Clear selectedWalletId so the header switch takes precedence over
      // any stale selection from WalletsView.
      selectedWalletId.set(null);
      activeWallet.set(w);
      open = false;
    } catch (e) {
      alert(e.message);
    }
  }

  function toggle() {
    open = !open;
  }

  function handleClickOutside(e) {
    if (open && !e.target.closest('.wallet-selector')) {
      open = false;
    }
  }

  function shorten(addr) {
    if (!addr) return '';
    return addr.slice(0, 6) + '...' + addr.slice(-4);
  }

  onMount(() => {
    loadWallets();
    window.addEventListener('click', handleClickOutside);
  });

  import { onDestroy } from 'svelte';
  onDestroy(() => window.removeEventListener('click', handleClickOutside));
</script>

<div class="wallet-selector relative">
  <button
    class="flex items-center gap-2 rounded-lg border border-gray-600 bg-surface px-3 py-1.5 text-sm transition-colors hover:bg-surface-dark"
    on:click={toggle}
  >
    {#if $activeWallet}
      <div class="flex h-6 w-6 items-center justify-center rounded-full {$activeWallet.source === 'kmd' ? 'bg-algo-500/10' : 'bg-blue-500/10'}">
        {#if $activeWallet.source === 'kmd'}
          <svg class="h-4 w-4 text-algo-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
          </svg>
        {:else}
          <svg class="h-4 w-4 text-blue-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5" />
          </svg>
        {/if}
      </div>
      <span class="max-w-[120px] truncate font-medium text-gray-200">{$activeWallet.name}</span>
    {:else}
      <svg class="h-4 w-4 text-gray-500" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
      </svg>
      <span class="text-gray-500">{$t('wallet.noActive')}</span>
    {/if}
    <svg class="h-4 w-4 text-gray-500 transition-transform {open ? 'rotate-180' : ''}" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
    </svg>
  </button>

  {#if open}
    <div class="absolute right-0 top-full z-40 mt-1 w-72 rounded-lg border border-gray-700 bg-surface-dark shadow-xl">
      {#if $walletList.length === 0}
        <div class="px-4 py-6 text-center text-sm text-gray-500">
          {$t('wallet.empty')}
        </div>
      {:else}
        <div class="max-h-64 overflow-y-auto py-1">
          {#each $walletList as w (w.id)}
            <button
              class="flex w-full items-center gap-3 px-3 py-2 text-left transition-colors hover:bg-gray-700/30
                {$activeWallet?.id === w.id ? 'bg-algo-500/5' : ''}"
              on:click={() => switchWallet(w)}
            >
              <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full {w.source === 'kmd' ? 'bg-algo-500/10' : 'bg-blue-500/10'}">
                {#if w.source === 'kmd'}
                  <svg class="h-4 w-4 text-algo-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
                  </svg>
                {:else}
                  <svg class="h-4 w-4 text-blue-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5" />
                  </svg>
                {/if}
              </div>
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-1.5">
                  <span class="truncate text-sm font-medium text-gray-200">{w.name}</span>
                  {#if $activeWallet?.id === w.id}
                    <span class="shrink-0 rounded-full bg-green-500/10 px-1.5 py-0.5 text-[10px] text-green-400">{$t('wallet.active')}</span>
                  {/if}
                </div>
                <p class="truncate font-mono text-xs text-gray-500">{shorten(w.first_address)}</p>
              </div>
            </button>
          {/each}
        </div>
      {/if}
      <div class="border-t border-gray-700/50 p-2">
        <button
          class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-gray-400 transition-colors hover:bg-gray-700/30 hover:text-gray-200"
          on:click={() => { open = false; activeView.set('wallets'); }}
        >
          <svg class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
          </svg>
          {$t('wallet.manageInWallets')}
        </button>
      </div>
    </div>
  {/if}
</div>
