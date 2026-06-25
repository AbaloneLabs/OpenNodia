<script>
  import { onMount, onDestroy } from 'svelte';
  import { createEventDispatcher } from 'svelte';
  import { api } from '../api.js';
  import { t } from '../i18n/index.js';
  import { activeView, parseOrderLinkHash, pendingOrderLinkPayload } from '../nav.js';
  import { setTradeNetwork } from '../tradeState.js';
  import LanguageSelector from './LanguageSelector.svelte';
  import Sidebar from './Sidebar.svelte';
  import WalletSelector from './WalletSelector.svelte';
  import NodeView from './NodeView.svelte';
  import WalletsView from './WalletsView.svelte';
  import AssetsView from './AssetsView.svelte';
  import DEXView from './DEXView.svelte';
  import LPTradeView from './LPTradeView.svelte';
  import AsaIssueView from './AsaIssueView.svelte';
  import SettingsView from './SettingsView.svelte';

  const dispatch = createEventDispatcher();

  export let status = null;

  let nodeStatus = null;
  let loading = true;
  let error = '';
  let refreshTimer;
  let algoPrice = null;
  let priceTimer;
  let syncProgress = null;
  let eventConnection = null;

  async function loadNodeStatus() {
    try {
      nodeStatus = await api.getNodeStatus();
      error = '';
      // Also fetch sync progress (non-critical, don't block on error)
      syncProgress = await api.getSyncProgress().catch(() => null);
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function loadPrice() {
    try {
      const resp = await api.getPrice();
      if (resp?.price_usd != null && resp.available) {
        algoPrice = resp.price_usd;
      }
    } catch (e) {
      // Price is non-critical, ignore errors silently
    }
  }

  onMount(() => {
    handleHashRoute();
    window.addEventListener('hashchange', handleHashRoute);
    loadNodeStatus();
    refreshTimer = setInterval(loadNodeStatus, 5000);
    loadPrice();
    priceTimer = setInterval(loadPrice, 60000); // refresh price every minute
    eventConnection = api.connectEvents({
      onEvent: ({ event, data }) => {
        if (event === 'node') {
          nodeStatus = {
            last_round: data.last_round,
            last_version: data.last_version,
            time_since_last_round: data.time_since_last_round,
            catchup_time: data.catchup_time,
            source: data.source,
          };
          syncProgress = data.sync_progress;
          error = '';
          loading = false;
        } else if (event === 'wallet_balance') {
          window.dispatchEvent(new CustomEvent('opennodia-wallet-balance', { detail: data }));
        }
      },
    });
  });

  onDestroy(() => {
    window.removeEventListener('hashchange', handleHashRoute);
    clearInterval(refreshTimer);
    clearInterval(priceTimer);
    eventConnection?.close();
  });

  $: network = status?.network || nodeStatus?.network || 'local';
  $: setTradeNetwork(network);
  $: isSynced = nodeStatus?.catchup_time === 0;

  function formatEta(seconds) {
    if (seconds == null) return null;
    if (seconds === 0) return null;
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
    const hours = Math.floor(seconds / 3600);
    const mins = Math.round((seconds % 3600) / 60);
    return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`;
  }

  function handleHashRoute() {
    const payload = parseOrderLinkHash(window.location.hash || '');
    if (!payload) return;
    pendingOrderLinkPayload.set(payload);
    activeView.set('dex');
  }

  $: etaLabel = !isSynced ? formatEta(syncProgress?.estimated_seconds_remaining) : null;
</script>

<div class="flex h-screen overflow-hidden">
  <!-- Sidebar (desktop) -->
  <Sidebar {network} {algoPrice} />

  <!-- Main column -->
  <div class="flex min-w-0 flex-1 flex-col">
    <!-- Header -->
    <header class="flex shrink-0 items-center justify-between border-b border-gray-700/50 bg-surface-dark/80 px-4 py-3 backdrop-blur sm:px-6">
      <div class="flex items-center gap-3">
        <!-- Mobile logo -->
        <img src="/opennodia-logo.svg" alt="OpenNodia" class="h-7 w-7 lg:hidden" />
        <span class="text-lg font-bold text-gray-100 lg:hidden">OpenNodia</span>
        <!-- Global wallet selector (account switcher) -->
        <WalletSelector />
      </div>
      <div class="flex items-center gap-2">
        {#if nodeStatus}
          <span class="hidden items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium {isSynced ? 'bg-green-500/10 text-green-400' : 'bg-yellow-500/10 text-yellow-400'} sm:flex">
            <span class="h-1.5 w-1.5 rounded-full {isSynced ? 'bg-green-400' : 'bg-yellow-400'} animate-pulse"></span>
            {isSynced ? $t('dashboard.synced') : $t('dashboard.catchingUp')}
            {#if etaLabel}<span class="ml-1 text-gray-400">({etaLabel})</span>{/if}
          </span>
        {/if}
        <LanguageSelector />
        <button class="btn-secondary px-3 py-1.5 text-sm" on:click={() => dispatch('logout')}>
          {$t('common.lock')}
        </button>
      </div>
    </header>

    <!-- Scrollable content -->
    <main class="flex-1 overflow-y-auto">
      <div class="mx-auto max-w-5xl px-4 py-8 pb-24 sm:px-6 lg:pb-8">
        {#if $activeView === 'node'}
          <NodeView {nodeStatus} {syncProgress} {loading} {error} {loadNodeStatus} />
        {:else if $activeView === 'wallets'}
          <WalletsView />
        {:else if $activeView === 'assets'}
          <AssetsView />
        {:else if $activeView === 'dex'}
          <DEXView />
        {:else if $activeView === 'lp'}
          <LPTradeView />
        {:else if $activeView === 'asa'}
          <AsaIssueView />
        {:else if $activeView === 'settings'}
          <SettingsView />
        {/if}
      </div>
    </main>
  </div>
</div>
