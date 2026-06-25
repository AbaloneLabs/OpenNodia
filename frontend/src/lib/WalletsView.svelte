<script>
  import { onDestroy, onMount } from 'svelte';
  import { api } from '../api.js';
  import { t } from '../i18n/index.js';
  import { activeView, selectedWalletId } from '../nav.js';
  import { activeWallet, walletList } from '../walletStore.js';
  import WalletModal from './WalletModal.svelte';

  let loading = true;
  let error = '';
  let showWalletModal = false;
  let modalMode = 'create';
  let portfolioValues = new Map();
  let portfolioLoading = false;

  // Inline rename state
  let editingId = null;
  let editName = '';
  let copiedId = null;
  let copiedTimer;

  async function loadWallets() {
    loading = true;
    error = '';
    try {
      const wallets = await api.listWallets();
      walletList.set(wallets);
      const active = await api.getActiveWallet();
      activeWallet.set(active.wallet);
      loadPortfolioValues();
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function activateWallet(w) {
    try {
      await api.activateWallet(w.id);
      activeWallet.set(w);
    } catch (e) {
      alert(e.message);
    }
  }

  async function removeWallet(w) {
    if (!confirm($t('wallet.removeConfirm', { name: w.name }))) return;
    try {
      await api.removeWallet(w.id);
      await loadWallets();
    } catch (e) {
      alert(e.message);
    }
  }

  function startRename(w) {
    editingId = w.id;
    editName = w.name;
  }

  async function saveRename(w) {
    if (!editName.trim()) {
      editingId = null;
      return;
    }
    try {
      await api.renameWallet(w.id, editName.trim());
      await loadWallets();
    } catch (e) {
      alert(e.message);
    }
    editingId = null;
  }

  function cancelRename() {
    editingId = null;
  }

  async function copyAddress(w) {
    try {
      await navigator.clipboard.writeText(w.first_address);
      copiedId = w.id;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copiedId = null), 2000);
    } catch (e) {
      // Fallback for older browsers
      const textarea = document.createElement('textarea');
      textarea.value = w.first_address;
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand('copy');
      document.body.removeChild(textarea);
      copiedId = w.id;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copiedId = null), 2000);
    }
  }

  function viewAssets(w) {
    selectedWalletId.set(w.id);
    activeWallet.set(w);
    activeView.set('assets');
  }

  function openCreate() {
    modalMode = 'create';
    showWalletModal = true;
  }

  function openImport() {
    modalMode = 'import';
    showWalletModal = true;
  }

  function shorten(addr) {
    if (!addr) return '';
    return addr.slice(0, 6) + '...' + addr.slice(-6);
  }

  async function loadPortfolioValues() {
    portfolioLoading = true;
    try {
      const result = await api.getWalletPortfolioValues('1d');
      portfolioValues = new Map((result.wallets || []).map((value) => [value.wallet_id, value]));
    } catch {
      portfolioValues = new Map();
    } finally {
      portfolioLoading = false;
    }
  }

  function walletValue(w) {
    return portfolioValues.get(w.id);
  }

  function formatUsd(value) {
    if (value == null || !Number.isFinite(Number(value))) return '—';
    return Number(value).toLocaleString(undefined, {
      style: 'currency',
      currency: 'USD',
      maximumFractionDigits: value >= 100 ? 0 : 4,
    });
  }

  function handleRealtimeBalance() {
    loadWallets();
  }

  onMount(() => {
    loadWallets();
    window.addEventListener('opennodia-wallet-balance', handleRealtimeBalance);
  });

  onDestroy(() => {
    window.removeEventListener('opennodia-wallet-balance', handleRealtimeBalance);
  });
</script>

<div class="mb-10">
  <div class="mb-4 flex items-center justify-between">
    <h2 class="text-lg font-semibold text-gray-200">{$t('wallet.title')}</h2>
    <div class="flex gap-2">
      <button class="btn-secondary px-3 py-1.5 text-sm" on:click={openImport}>
        {$t('wallet.import')}
      </button>
      <button class="btn-primary px-3 py-1.5 text-sm" on:click={openCreate}>
        {$t('wallet.create')}
      </button>
    </div>
  </div>

  {#if loading}
    <div class="card flex items-center justify-center py-12">
      <div class="animate-pulse text-algo-500">
        <svg class="h-8 w-8" viewBox="0 0 100 100" fill="none" stroke="currentColor" stroke-width="4">
          <circle cx="50" cy="50" r="40" stroke-dasharray="60" stroke-linecap="round" />
        </svg>
      </div>
    </div>
  {:else if error}
    <div class="card text-center">
      <p class="text-red-400">{error}</p>
    </div>
  {:else if $walletList.length === 0}
    <div class="card flex items-center justify-center py-16 text-center">
      <div>
        <svg class="mx-auto mb-3 h-10 w-10 text-gray-600" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" d="M21 12a2.25 2.25 0 00-2.25-2.25H15a3 3 0 11-6 0H5.25A2.25 2.25 0 003 12m18 0v6a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 18v-6m18 0V9M3 12V9m18 0a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 9m18 0V6a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 6v3" />
        </svg>
        <p class="text-sm text-gray-500">{$t('wallet.empty')}</p>
        <p class="mt-1 text-xs text-gray-600">{$t('wallet.emptyHint')}</p>
      </div>
    </div>
  {:else}
    <div class="space-y-2">
      {#each $walletList as w (w.id)}
        <div
          class="card flex items-center justify-between transition-colors {$activeWallet?.id === w.id ? 'border-algo-500/50 bg-algo-500/5' : ''}"
        >
          <div class="flex min-w-0 flex-1 items-center gap-3">
            <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full {w.source === 'kmd' ? 'bg-algo-500/10' : 'bg-blue-500/10'}">
              {#if w.source === 'kmd'}
                <svg class="h-5 w-5 text-algo-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
                </svg>
              {:else}
                <svg class="h-5 w-5 text-blue-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5" />
                </svg>
              {/if}
            </div>
            <div class="min-w-0 flex-1">
              {#if editingId === w.id}
                <div class="flex items-center gap-2">
                  <input
                    class="input-sm"
                    bind:value={editName}
                    on:keydown={(e) => {
                      if (e.key === 'Enter') saveRename(w);
                      if (e.key === 'Escape') cancelRename();
                    }}
                  />
                  <button class="text-green-400 hover:text-green-300" on:click={() => saveRename(w)} aria-label={$t('common.save')}>
                    <svg class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M4.5 12.75l6 6 9-13.5" />
                    </svg>
                  </button>
                  <button class="text-gray-500 hover:text-gray-300" on:click={cancelRename} aria-label={$t('common.cancel')}>
                    <svg class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </div>
              {:else}
                <button
                  class="group flex items-center gap-1.5 text-left"
                  on:click={() => viewAssets(w)}
                  title={$t('wallet.viewAssets')}
                >
                  <span class="font-medium text-gray-200 hover:text-algo-400">{w.name}</span>
                  <span class="rounded-full px-2 py-0.5 text-xs {w.source === 'kmd' ? 'bg-algo-500/10 text-algo-400' : 'bg-blue-500/10 text-blue-400'}">
                    {w.source}
                  </span>
                  {#if $activeWallet?.id === w.id}
                    <span class="rounded-full bg-green-500/10 px-2 py-0.5 text-xs text-green-400">{$t('wallet.active')}</span>
                  {/if}
                </button>
                <div class="mt-0.5 flex items-center gap-2">
                  <p class="font-mono text-xs text-gray-500">{shorten(w.first_address)}</p>
                  <button
                    class="text-gray-600 transition-colors hover:text-algo-400"
                    on:click={() => copyAddress(w)}
                    title={$t('wallet.copyAddress')}
                  >
                    {#if copiedId === w.id}
                      <svg class="h-3.5 w-3.5 text-green-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M4.5 12.75l6 6 9-13.5" />
                      </svg>
                    {:else}
                      <svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M15.75 17.25v3.375c0 .621-.504 1.125-1.125 1.125h-9.75a1.125 1.125 0 01-1.125-1.125V7.875c0-.621.504-1.125 1.125-1.125H6.75a9.06 9.06 0 011.5.124m7.5 10.376h3.375c.621 0 1.125-.504 1.125-1.125V11.25c0-4.46-3.243-8.161-7.5-8.876a9.06 9.06 0 00-1.5-.124H9.375c-.621 0-1.125.504-1.125 1.125v3.5m7.5 10.375H9.375a1.125 1.125 0 01-1.125-1.125v-9.25m12 6.625v-1.875a3.375 3.375 0 00-3.375-3.375h-1.5a1.125 1.125 0 01-1.125-1.125v-1.5a3.375 3.375 0 00-3.375-3.375H9.75" />
                      </svg>
                    {/if}
                  </button>
                </div>
              {/if}
            </div>
          </div>
          <div class="flex shrink-0 items-center gap-3">
            <div class="hidden min-w-[7rem] text-right sm:block">
              <p class="font-mono text-sm text-gray-200">{portfolioLoading ? '—' : formatUsd(walletValue(w)?.current?.total_value_usd)}</p>
              {#if walletValue(w)?.current?.unpriced_asset_count > 0}
                <p class="text-xs text-gray-600">{$t('wallet.unpricedCount', { count: walletValue(w).current.unpriced_asset_count })}</p>
              {:else}
                <p class="text-xs text-gray-600">{$t('wallet.totalValue')}</p>
              {/if}
            </div>
            <div class="flex gap-1">
            {#if editingId !== w.id}
              <button
                class="rounded-lg p-2 text-gray-500 hover:bg-gray-700/50 hover:text-gray-300"
                on:click={() => startRename(w)}
                title={$t('wallet.rename')}
              >
                <svg class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M16.862 4.487l1.687-1.688a1.875 1.875 0 112.652 2.652L10.582 16.07a4.5 4.5 0 01-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 011.13-1.897l8.932-8.931zm0 0L19.5 7.125" />
                </svg>
              </button>
              {#if $activeWallet?.id !== w.id}
                <button
                  class="rounded-lg px-3 py-1.5 text-xs text-gray-400 hover:bg-gray-700/50 hover:text-gray-200"
                  on:click={() => activateWallet(w)}
                >
                  {$t('wallet.activate')}
                </button>
              {/if}
              <button
                class="rounded-lg p-2 text-gray-500 hover:bg-red-500/10 hover:text-red-400"
                on:click={() => removeWallet(w)}
                title={$t('wallet.remove')}
              >
                <svg class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M14.74 9l-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 01-2.244 2.077H8.084a2.25 2.25 0 01-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 00-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 013.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 00-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 00-7.5 0" />
                </svg>
              </button>
            {/if}
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

{#if showWalletModal}
  <WalletModal
    mode={modalMode}
    on:close={() => (showWalletModal = false)}
    on:created={async () => {
      showWalletModal = false;
      await loadWallets();
    }}
  />
{/if}
