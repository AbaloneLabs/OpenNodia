<script>
  import { onDestroy, onMount } from 'svelte';
  import { api } from '../api.js';
  import { t } from '../i18n/index.js';
  import { selectedWalletId, activeView } from '../nav.js';
  import { walletList, activeWallet } from '../walletStore.js';
  import {
    ASSET_COLOR_LABELS,
    availableTags,
    computeAssetDisplay,
    emptyAssetMetadata,
    getAssetMetadata,
    metadataArrayToMap,
    setAssetMetadataInMap,
  } from '../assetMetadata.js';
  import TransferModal from './TransferModal.svelte';
  import OptInModal from './OptInModal.svelte';
  import ReceiveModal from './ReceiveModal.svelte';

  let accountAssets = null;
  let accountInfo = null;
  let assetMetadataMap = new Map();
  let loading = false;
  let error = '';
  let metadataError = '';
  let sendAsset = null;
  let metadataEditAsset = null;
  let metadataForm = emptyAssetMetadata(0);
  let metadataSaving = false;
  let showOptInModal = false;
  let showReceiveModal = false;
  let loadedAddress = '';

  // Transaction history (requires indexer)
  let txHistory = [];
  let txHistoryTotal = 0;
  let txHistoryLoading = false;
  let txHistoryError = '';
  let showTxHistory = false;
  let indexerAvailable = null;
  let txLimit = 30;
  let txOffset = 0;
  let txTypeFilter = 'all';
  let txAssetFilter = 'all';
  let txFromDate = '';
  let txToDate = '';
  let txExporting = false;
  let balanceSnapshots = [];
  let snapshotsLoading = false;
  let snapshotsError = '';
  let portfolio = null;
  let portfolioRange = '1m';
  let portfolioLoading = false;
  let portfolioError = '';
  let portfolioAssetMap = new Map();
  let assetAnalytics = null;
  let analyticsLoading = false;
  let analyticsError = '';
  let analyticsAssetId = '';
  let analyticsMinRound = '';
  let analyticsMaxRound = '';
  let analyticsPolicy = 'all';

  // Sort: 'balance-desc' | 'balance-asc' | 'name'
  let sortBy = 'balance-desc';
  let tagFilter = 'all';
  let policyFilter = 'all';
  let balanceFilter = 'all';
  let tagOptions = [];

  // Policy info popover (fixed-position overlay to avoid table overflow clipping)
  let showPolicyInfo = false;
  let policyInfoPos = { x: 0, y: 0 };

  function togglePolicyInfo(event) {
    showPolicyInfo = !showPolicyInfo;
    if (showPolicyInfo) {
      const rect = event.currentTarget.getBoundingClientRect();
      policyInfoPos = { x: rect.left, y: rect.bottom + 8 };
    }
  }

  async function loadWallets() {
    try {
      const wallets = await api.listWallets();
      walletList.set(wallets);
      if (!$activeWallet) {
        const active = await api.getActiveWallet();
        activeWallet.set(active.wallet);
      }
    } catch (e) {
      // ignore
    }
  }

  async function loadAccount() {
    if (!currentWallet) {
      accountAssets = null;
      accountInfo = null;
      return;
    }
    loading = true;
    error = '';
    try {
      const [assets, info, metadata, portfolioResult] = await Promise.all([
        api.getAccountAssets(currentWallet.first_address),
        api.getAccount(currentWallet.first_address),
        api.listAssetMetadata(currentWallet.first_address),
        api.getPortfolioValue(currentWallet.first_address, portfolioRange),
      ]);
      accountAssets = assets;
      accountInfo = info;
      assetMetadataMap = metadataArrayToMap(metadata.metadata);
      portfolio = portfolioResult;
      portfolioAssetMap = new Map((portfolioResult.assets || []).map((asset) => [String(asset.asset_id), asset]));
      portfolioError = '';
      metadataError = '';
    } catch (e) {
      error = e.message;
      accountAssets = null;
      accountInfo = null;
      assetMetadataMap = new Map();
      portfolio = null;
      portfolioAssetMap = new Map();
    } finally {
      loading = false;
    }
  }

  function formatAmount(amount, decimals) {
    const d = decimals || 0;
    return (amount / Math.pow(10, d)).toLocaleString(undefined, {
      minimumFractionDigits: 0,
      maximumFractionDigits: d,
    });
  }

  function shorten(addr) {
    if (!addr) return '';
    return addr.slice(0, 8) + '...' + addr.slice(-8);
  }

  function policyColor(policy) {
    switch (policy) {
      case 'open':
        return 'text-green-400 bg-green-500/10';
      case 'bridged':
        return 'text-yellow-400 bg-yellow-500/10';
      case 'regulated':
        return 'text-red-400 bg-red-500/10';
      default:
        return 'text-gray-400 bg-gray-500/10';
    }
  }

  $: displayAssets = computeAssetDisplay(accountAssets, assetMetadataMap, sortBy, {
    tag: tagFilter,
    policy: policyFilter,
    balance: balanceFilter,
  });
  $: tagOptions = availableTags(assetMetadataMap);
  $: txAssetOptions = accountAssets?.assets || [];
  $: analyticsAssetOptions = (accountAssets?.assets || []).filter((asset) => asset.id !== 0);
  $: if (!analyticsAssetId && analyticsAssetOptions.length > 0) {
    analyticsAssetId = String(analyticsAssetOptions[0].id);
  }

  $: currentWallet = $walletList.find((w) => w.id === $selectedWalletId) || $activeWallet;

  onMount(async () => {
    await loadWallets();
    checkIndexer();
    window.addEventListener('opennodia-wallet-balance', handleRealtimeBalance);
  });

  onDestroy(() => {
    window.removeEventListener('opennodia-wallet-balance', handleRealtimeBalance);
  });

  function closeTransferModals() {
    sendAsset = null;
    showOptInModal = false;
    showReceiveModal = false;
  }

  function openSend(asset) {
    if (!asset || asset.amount <= 0 || asset.frozen) return;
    sendAsset = asset;
  }

  function assetMeta(assetId) {
    return getAssetMetadata(assetMetadataMap, assetId);
  }

  function colorDotClass(label) {
    switch (label) {
      case 'red':
        return 'bg-red-400';
      case 'orange':
        return 'bg-orange-400';
      case 'yellow':
        return 'bg-yellow-400';
      case 'green':
        return 'bg-green-400';
      case 'cyan':
        return 'bg-cyan-400';
      case 'blue':
        return 'bg-blue-400';
      case 'purple':
        return 'bg-purple-400';
      case 'pink':
        return 'bg-pink-400';
      default:
        return 'bg-slate-400';
    }
  }

  function openMetadataEditor(asset) {
    metadataError = '';
    metadataEditAsset = asset;
    metadataForm = { ...assetMeta(asset.id) };
  }

  function closeMetadataEditor() {
    metadataEditAsset = null;
    metadataSaving = false;
  }

  async function saveMetadataForm() {
    if (!currentWallet || !metadataEditAsset) return;
    metadataSaving = true;
    metadataError = '';
    try {
      const saved = await api.saveAssetMetadata(currentWallet.first_address, metadataEditAsset.id, metadataForm);
      assetMetadataMap = setAssetMetadataInMap(assetMetadataMap, saved);
      closeMetadataEditor();
    } catch (e) {
      metadataError = e.message;
    } finally {
      metadataSaving = false;
    }
  }

  async function clearMetadataForm() {
    if (!currentWallet || !metadataEditAsset) return;
    metadataSaving = true;
    metadataError = '';
    try {
      await api.clearAssetMetadata(currentWallet.first_address, metadataEditAsset.id);
      assetMetadataMap = setAssetMetadataInMap(assetMetadataMap, emptyAssetMetadata(metadataEditAsset.id));
      closeMetadataEditor();
    } catch (e) {
      metadataError = e.message;
    } finally {
      metadataSaving = false;
    }
  }

  async function toggleAssetPin(asset) {
    if (!currentWallet || !asset) return;
    metadataError = '';
    const current = assetMeta(asset.id);
    try {
      const saved = await api.saveAssetMetadata(currentWallet.first_address, asset.id, {
        ...current,
        pinned: !current.pinned,
      });
      assetMetadataMap = setAssetMetadataInMap(assetMetadataMap, saved);
    } catch (e) {
      metadataError = e.message;
    }
  }

  async function handleTransferCompleted() {
    await loadAccount();
  }

  function handleRealtimeBalance(event) {
    if (!currentWallet || event.detail?.address !== currentWallet.first_address) return;
    loadAccount();
    if (showTxHistory) {
      loadTxHistory(0);
      loadBalanceSnapshots();
    }
  }

  async function checkIndexer() {
    try {
      const status = await api.getIndexerStatus();
      indexerAvailable = status.available;
    } catch {
      indexerAvailable = false;
    }
  }

  function dateToUnixStart(value) {
    if (!value) return null;
    const time = Date.parse(value + 'T00:00:00');
    return Number.isFinite(time) ? Math.floor(time / 1000) : null;
  }

  function dateToUnixEnd(value) {
    if (!value) return null;
    const time = Date.parse(value + 'T23:59:59');
    return Number.isFinite(time) ? Math.floor(time / 1000) : null;
  }

  function txQuery(offset = txOffset) {
    return {
      limit: txLimit,
      offset,
      txType: txTypeFilter,
      assetId: txAssetFilter,
      fromTime: dateToUnixStart(txFromDate),
      toTime: dateToUnixEnd(txToDate),
    };
  }

  async function loadTxHistory(offset = txOffset) {
    if (!currentWallet) return;
    txHistoryLoading = true;
    txHistoryError = '';
    try {
      const result = await api.getAccountTransactions(currentWallet.first_address, txQuery(offset));
      txHistory = result.transactions || [];
      txHistoryTotal = result.total || 0;
      txOffset = result.offset || 0;
    } catch (e) {
      txHistoryError = e.message;
      txHistory = [];
      txHistoryTotal = 0;
    } finally {
      txHistoryLoading = false;
    }
  }

  function toggleTxHistory() {
    showTxHistory = !showTxHistory;
    if (showTxHistory && txHistory.length === 0 && !txHistoryError) {
      loadTxHistory(0);
      loadBalanceSnapshots();
    }
  }

  function applyTxFilters() {
    loadTxHistory(0);
  }

  function previousTxPage() {
    loadTxHistory(Math.max(0, txOffset - txLimit));
  }

  function nextTxPage() {
    if (txOffset + txLimit < txHistoryTotal) {
      loadTxHistory(txOffset + txLimit);
    }
  }

  async function exportTxCsv() {
    if (!currentWallet) return;
    txExporting = true;
    txHistoryError = '';
    try {
      const csv = await api.exportAccountTransactionsCsv(currentWallet.first_address, {
        ...txQuery(0),
        limit: 5000,
      });
      const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `opennodia-${currentWallet.first_address.slice(0, 8)}-transactions.csv`;
      document.body.appendChild(link);
      link.click();
      link.remove();
      URL.revokeObjectURL(url);
    } catch (e) {
      txHistoryError = e.message;
    } finally {
      txExporting = false;
    }
  }

  async function loadBalanceSnapshots() {
    if (!currentWallet) return;
    snapshotsLoading = true;
    snapshotsError = '';
    try {
      const result = await api.getBalanceSnapshots(currentWallet.first_address, 12);
      balanceSnapshots = result.snapshots || [];
    } catch (e) {
      snapshotsError = e.message;
      balanceSnapshots = [];
    } finally {
      snapshotsLoading = false;
    }
  }

  async function loadPortfolio(range = portfolioRange) {
    if (!currentWallet) return;
    portfolioLoading = true;
    portfolioError = '';
    try {
      const result = await api.getPortfolioValue(currentWallet.first_address, range);
      portfolio = result;
      portfolioRange = result.range || range;
      portfolioAssetMap = new Map((result.assets || []).map((asset) => [String(asset.asset_id), asset]));
    } catch (e) {
      portfolioError = e.message;
      portfolio = null;
      portfolioAssetMap = new Map();
    } finally {
      portfolioLoading = false;
    }
  }

  function portfolioAsset(assetId) {
    return portfolioAssetMap.get(String(assetId));
  }

  function formatUsd(value) {
    if (value == null || !Number.isFinite(Number(value))) return '—';
    return Number(value).toLocaleString(undefined, {
      style: 'currency',
      currency: 'USD',
      maximumFractionDigits: value >= 100 ? 0 : 4,
    });
  }

  function formatAlgoPrice(value) {
    if (value == null || !Number.isFinite(Number(value))) return '—';
    return `${Number(value).toLocaleString(undefined, { maximumFractionDigits: 8 })} ALGO`;
  }

  function formatPercent(value) {
    if (value == null || !Number.isFinite(Number(value))) return '—';
    const sign = value > 0 ? '+' : '';
    return `${sign}${Number(value).toFixed(2)}%`;
  }

  function portfolioChartPoints() {
    const points = portfolio?.history || [];
    if (points.length < 2) return '';
    const values = points.map((point) => Number(point.total_value_usd)).filter(Number.isFinite);
    if (values.length < 2) return '';
    const min = Math.min(...values);
    const max = Math.max(...values);
    const span = max - min || 1;
    return points
      .map((point, index) => {
        const x = (index / Math.max(points.length - 1, 1)) * 100;
        const y = 36 - ((Number(point.total_value_usd) - min) / span) * 32;
        return `${x},${Math.max(2, Math.min(38, y))}`;
      })
      .join(' ');
  }

  function analyticsQuery() {
    return {
      limit: 25,
      minRound: analyticsMinRound || null,
      maxRound: analyticsMaxRound || null,
      policy: analyticsPolicy,
    };
  }

  async function loadAssetAnalytics() {
    if (!analyticsAssetId) return;
    analyticsLoading = true;
    analyticsError = '';
    try {
      const [transactions, holders, applications] = await Promise.all([
        api.getAssetTransactionsAnalysis(analyticsAssetId, analyticsQuery()),
        api.getAssetHolders(analyticsAssetId, { limit: 10 }),
        api.getAssetApplications(analyticsAssetId, { limit: 10 }),
      ]);
      assetAnalytics = { transactions, holders, applications };
    } catch (e) {
      analyticsError = e.message;
      assetAnalytics = null;
    } finally {
      analyticsLoading = false;
    }
  }

  function txTypeLabel(type) {
    const map = {
      pay: 'txTypePay',
      axfer: 'txTypeAxfer',
      afrz: 'txTypeAfrz',
      keyreg: 'txTypeKeyreg',
      acfg: 'txTypeAcfg',
      appl: 'txTypeAppl',
    };
    return $t('assets.' + (map[type] || 'txTypeOther'));
  }

  function formatTime(unixSeconds) {
    if (!unixSeconds) return '';
    const d = new Date(unixSeconds * 1000);
    return d.toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  }

  function shortenTxid(txid) {
    if (!txid) return '';
    return txid.slice(0, 10) + '...' + txid.slice(-6);
  }

  // Reload once when the resolved wallet changes.
  $: if (currentWallet?.first_address && currentWallet.first_address !== loadedAddress) {
    loadedAddress = currentWallet.first_address;
    closeTransferModals();
    closeMetadataEditor();
    txHistory = [];
    txHistoryTotal = 0;
    txOffset = 0;
    txHistoryError = '';
    showTxHistory = false;
    balanceSnapshots = [];
    snapshotsError = '';
    portfolio = null;
    portfolioError = '';
    portfolioAssetMap = new Map();
    assetAnalytics = null;
    analyticsError = '';
    analyticsAssetId = '';
    loadAccount();
  }
</script>

<div class="mb-10">
  {#if !currentWallet}
    <!-- No wallet selected -->
    <div class="card flex flex-col items-center justify-center py-16 text-center">
      <svg class="mb-4 h-12 w-12 text-gray-600" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
      </svg>
      <p class="text-sm text-gray-500">{$t('assets.noWalletSelected')}</p>
      <button class="btn-primary mt-4 px-4 py-2 text-sm" on:click={() => activeView.set('wallets')}>
        {$t('assets.goToWallets')}
      </button>
    </div>
  {:else if loading}
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
      <button class="btn-secondary mt-4 px-4 py-2 text-sm" on:click={loadAccount}>
        {$t('common.retry')}
      </button>
    </div>
  {:else if accountAssets}
    <!-- Wallet header -->
    <div class="mb-6 flex flex-wrap items-start justify-between gap-3">
      <div>
        <div class="flex items-center gap-2">
          <h2 class="text-lg font-semibold text-gray-200">{currentWallet.name}</h2>
          <span class="rounded-full px-2 py-0.5 text-xs {currentWallet.source === 'kmd' ? 'bg-algo-500/10 text-algo-400' : 'bg-blue-500/10 text-blue-400'}">
            {currentWallet.source}
          </span>
        </div>
        <p class="mt-1 font-mono text-xs text-gray-500">{shorten(currentWallet.first_address)}</p>
      </div>
      <div class="flex gap-2">
        <button class="btn-secondary px-3 py-1.5 text-sm" on:click={() => (showReceiveModal = true)}>
          {$t('receive.button')}
        </button>
        <button class="btn-primary px-3 py-1.5 text-sm" on:click={() => (showOptInModal = true)}>
          {$t('optIn.button')}
        </button>
      </div>
    </div>

    <!-- Portfolio value -->
    <div class="mb-4 rounded-lg border border-gray-800 bg-gray-900/50 p-4">
      <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-xs uppercase tracking-wide text-gray-500">{$t('assets.portfolioValue')}</p>
          <div class="mt-1 flex flex-wrap items-end gap-2">
            <span class="text-2xl font-semibold text-gray-100">{formatUsd(portfolio?.current?.total_value_usd)}</span>
            <span class="pb-1 text-xs {portfolio?.current?.change_pct > 0 ? 'text-green-400' : portfolio?.current?.change_pct < 0 ? 'text-red-400' : 'text-gray-500'}">
              {formatPercent(portfolio?.current?.change_pct)}
            </span>
          </div>
        </div>
        <div class="flex items-center gap-2">
          <select
            bind:value={portfolioRange}
            on:change={() => loadPortfolio(portfolioRange)}
            class="rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none"
          >
            <option value="1d">1D</option>
            <option value="1w">1W</option>
            <option value="1m">1M</option>
            <option value="1y">1Y</option>
          </select>
          <button class="rounded border border-gray-700 px-2 py-1 text-xs text-gray-300 hover:border-gray-500" on:click={() => loadPortfolio(portfolioRange)} disabled={portfolioLoading}>
            {$t('common.refresh')}
          </button>
        </div>
      </div>
      {#if portfolioError}
        <p class="mb-3 text-sm text-red-400">{portfolioError}</p>
      {/if}
      <div class="grid gap-3 sm:grid-cols-3">
        <div class="rounded-md border border-gray-800 bg-gray-950/40 px-3 py-2">
          <p class="text-xs text-gray-500">{$t('assets.algoValue')}</p>
          <p class="mt-1 font-mono text-sm text-gray-200">{formatUsd(portfolio?.current?.algo_value_usd)}</p>
        </div>
        <div class="rounded-md border border-gray-800 bg-gray-950/40 px-3 py-2">
          <p class="text-xs text-gray-500">{$t('assets.asaValue')}</p>
          <p class="mt-1 font-mono text-sm text-gray-200">{formatUsd(portfolio?.current?.asa_value_usd)}</p>
        </div>
        <div class="rounded-md border border-gray-800 bg-gray-950/40 px-3 py-2">
          <p class="text-xs text-gray-500">{$t('assets.unpricedAssets')}</p>
          <p class="mt-1 font-mono text-sm text-gray-200">{portfolio?.current?.unpriced_asset_count ?? 0}</p>
        </div>
      </div>
      {#if portfolioChartPoints()}
        <svg class="mt-4 h-12 w-full overflow-visible" viewBox="0 0 100 40" preserveAspectRatio="none" aria-label={$t('assets.portfolioChart')}>
          <polyline points={portfolioChartPoints()} fill="none" stroke="rgb(45 212 191)" stroke-width="2" vector-effect="non-scaling-stroke" />
        </svg>
      {/if}
    </div>

    <!-- Toolbar: sort + filters -->
    <div class="mb-4 flex flex-wrap items-center gap-3">
      <!-- Sort dropdown -->
      <div class="flex items-center gap-2">
        <span class="text-xs text-gray-500">{$t('assets.sortBy')}</span>
        <select
          bind:value={sortBy}
          class="rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none"
        >
          <option value="balance-desc">{$t('assets.sortBalanceDesc')}</option>
          <option value="balance-asc">{$t('assets.sortBalanceAsc')}</option>
          <option value="name">{$t('assets.sortName')}</option>
        </select>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-xs text-gray-500">{$t('assets.filterTag')}</span>
        <select
          bind:value={tagFilter}
          class="rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none"
        >
          <option value="all">{$t('assets.filterAll')}</option>
          {#each tagOptions as tag}
            <option value={tag}>{tag}</option>
          {/each}
        </select>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-xs text-gray-500">{$t('assets.filterPolicy')}</span>
        <select
          bind:value={policyFilter}
          class="rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none"
        >
          <option value="all">{$t('assets.filterAll')}</option>
          <option value="open">{$t('assets.policy_open')}</option>
          <option value="bridged">{$t('assets.policy_bridged')}</option>
          <option value="regulated">{$t('assets.policy_regulated')}</option>
        </select>
      </div>
      <div class="flex items-center gap-2">
        <span class="text-xs text-gray-500">{$t('assets.filterBalance')}</span>
        <select
          bind:value={balanceFilter}
          class="rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none"
        >
          <option value="all">{$t('assets.filterAll')}</option>
          <option value="nonzero">{$t('assets.filterBalanceNonzero')}</option>
          <option value="zero">{$t('assets.filterBalanceZero')}</option>
        </select>
      </div>
    </div>
    {#if metadataError}
      <p class="mb-4 text-sm text-red-400">{metadataError}</p>
    {/if}

    <!-- Unified asset table -->
    <div class="overflow-x-auto rounded-xl border border-gray-800">
      <table class="w-full">
        <thead>
          <tr class="border-b border-gray-800 bg-gray-800/50">
            <th class="w-10 px-3 py-2"></th>
            <th class="px-3 py-2 text-left text-xs font-medium uppercase tracking-wide text-gray-500">{$t('assets.colName')}</th>
            <th class="px-3 py-2 text-left text-xs font-medium uppercase tracking-wide text-gray-500">{$t('assets.colId')}</th>
            <th class="px-3 py-2 text-right text-xs font-medium uppercase tracking-wide text-gray-500">{$t('assets.colBalance')}</th>
            <th class="px-3 py-2 text-right text-xs font-medium uppercase tracking-wide text-gray-500">{$t('assets.colValue')}</th>
            <th class="px-3 py-2 text-right text-xs font-medium uppercase tracking-wide text-gray-500">{$t('assets.colPrice')}</th>
            <th class="px-3 py-2 text-right text-xs font-medium uppercase tracking-wide text-gray-500">{$t('assets.colChange')}</th>
            <th class="px-3 py-2 text-right text-xs font-medium uppercase tracking-wide text-gray-500">
              <span class="inline-flex items-center gap-1">
                {$t('assets.colPolicy')}
                <button
                  type="button"
                  on:click={togglePolicyInfo}
                  class="text-gray-500 hover:text-gray-300"
                  aria-label="Policy info"
                >
                  <svg class="h-3.5 w-3.5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M9.879 7.519c1.171-1.025 3.071-1.025 4.242 0 1.172 1.025 1.172 2.687 0 3.712-.203.179-.43.326-.67.442-.745.361-1.45.999-1.45 1.827v.75M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-9 5.25h.008v.008H12v-.008Z" />
                  </svg>
                </button>
              </span>
            </th>
            <th class="px-3 py-2 text-right text-xs font-medium uppercase tracking-wide text-gray-500">
              {$t('assets.colActions')}
            </th>
          </tr>
        </thead>
        <tbody class="divide-y divide-gray-800/50">
          <!-- Tier 1: ALGO (always first, no star, no toggle) -->
          {#if displayAssets.algo}
            <tr class="bg-algo-500/5">
              <td class="px-3 py-3"></td>
              <td class="px-3 py-3">
                <div class="flex items-center gap-2">
                  <div class="flex h-8 w-8 items-center justify-center rounded-full bg-algo-500/20">
                    <span class="text-xs font-bold text-algo-400">A</span>
                  </div>
                  <div>
                    <p class="text-sm font-medium text-gray-200">{displayAssets.algo.name}</p>
                    <p class="text-xs text-gray-500">{displayAssets.algo.unit}</p>
                    {#if assetMeta(displayAssets.algo.id).tag || assetMeta(displayAssets.algo.id).memo || assetMeta(displayAssets.algo.id).color_label}
                      <div class="mt-1 flex max-w-xs flex-wrap items-center gap-1">
                        {#if assetMeta(displayAssets.algo.id).color_label}
                          <span class="h-2 w-2 rounded-full {colorDotClass(assetMeta(displayAssets.algo.id).color_label)}"></span>
                        {/if}
                        {#if assetMeta(displayAssets.algo.id).tag}
                          <span class="rounded border border-gray-700 px-1.5 py-0.5 text-[11px] text-gray-300">{assetMeta(displayAssets.algo.id).tag}</span>
                        {/if}
                        {#if assetMeta(displayAssets.algo.id).memo}
                          <span class="max-w-[16rem] truncate text-[11px] text-gray-500">{assetMeta(displayAssets.algo.id).memo}</span>
                        {/if}
                      </div>
                    {/if}
                  </div>
                </div>
              </td>
              <td class="px-3 py-3 font-mono text-xs text-gray-500">{$t('assets.native')}</td>
              <td class="px-3 py-3 text-right font-mono text-sm font-medium text-algo-400">
                {formatAmount(displayAssets.algo.amount, displayAssets.algo.decimals)}
              </td>
              <td class="px-3 py-3 text-right font-mono text-xs text-gray-300">{formatUsd(portfolioAsset(0)?.value_usd)}</td>
              <td class="px-3 py-3 text-right font-mono text-xs text-gray-300">{formatUsd(portfolioAsset(0)?.price_usd)}</td>
              <td class="px-3 py-3 text-right font-mono text-xs {portfolio?.current?.change_pct > 0 ? 'text-green-400' : portfolio?.current?.change_pct < 0 ? 'text-red-400' : 'text-gray-500'}">{formatPercent(portfolio?.current?.change_pct)}</td>
              <td class="px-3 py-3 text-right">
                <span class="inline-block rounded-full px-2 py-0.5 text-xs text-gray-400 bg-gray-500/10">
                  {$t('assets.policyNative')}
                </span>
              </td>
              <td class="px-3 py-3 text-right">
                <div class="flex justify-end gap-2">
                <button
                  class="rounded-lg border border-gray-700 px-3 py-1.5 text-xs text-algo-400 hover:border-algo-500/50 hover:bg-algo-500/5 disabled:cursor-not-allowed disabled:opacity-40"
                  on:click={() => openSend(displayAssets.algo)}
                  disabled={displayAssets.algo.amount <= 0}
                >
                  {$t('transfer.send')}
                </button>
                <button
                  class="rounded-lg border border-gray-700 px-2 py-1.5 text-gray-400 hover:border-gray-500 hover:text-gray-200"
                  on:click={() => openMetadataEditor(displayAssets.algo)}
                  title={$t('assets.editMetadata')}
                  aria-label={$t('assets.editMetadata')}
                >
                  <svg class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="1.7" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" d="m16.862 4.487 1.687-1.688a1.875 1.875 0 1 1 2.652 2.652L10.582 16.07a4.5 4.5 0 0 1-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 0 1 1.13-1.897l8.932-8.931Z" />
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 7.125 16.875 4.5" />
                  </svg>
                </button>
                </div>
              </td>
            </tr>
          {/if}

          <!-- Tier 2: Pinned ASAs -->
          {#each displayAssets.pinned as asset (asset.id)}
            <tr class="bg-gray-800/30">
              <td class="px-3 py-3 text-center">
                <button
                  class="text-yellow-400 hover:text-yellow-300"
                  on:click={() => toggleAssetPin(asset)}
                  title={$t('assets.unpin')}
                >
                  <svg class="h-4 w-4" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
                  </svg>
                </button>
              </td>
              <td class="px-3 py-3">
                <div class="flex items-center gap-2">
                  <div class="flex h-8 w-8 items-center justify-center rounded-full bg-gray-700/50">
                    <span class="text-xs font-bold text-gray-400">{(asset.unit || asset.name || '?').charAt(0).toUpperCase()}</span>
                  </div>
                  <div>
                    <p class="text-sm font-medium text-gray-200">{asset.name || $t('assets.unknown')}</p>
                    <p class="text-xs text-gray-500">{asset.unit}</p>
                    {#if assetMeta(asset.id).tag || assetMeta(asset.id).memo || assetMeta(asset.id).color_label}
                      <div class="mt-1 flex max-w-xs flex-wrap items-center gap-1">
                        {#if assetMeta(asset.id).color_label}
                          <span class="h-2 w-2 rounded-full {colorDotClass(assetMeta(asset.id).color_label)}"></span>
                        {/if}
                        {#if assetMeta(asset.id).tag}
                          <span class="rounded border border-gray-700 px-1.5 py-0.5 text-[11px] text-gray-300">{assetMeta(asset.id).tag}</span>
                        {/if}
                        {#if assetMeta(asset.id).memo}
                          <span class="max-w-[16rem] truncate text-[11px] text-gray-500">{assetMeta(asset.id).memo}</span>
                        {/if}
                      </div>
                    {/if}
                  </div>
                </div>
              </td>
              <td class="px-3 py-3 font-mono text-xs text-gray-500">{asset.id}</td>
              <td class="px-3 py-3 text-right font-mono text-sm text-gray-300">
                {formatAmount(asset.amount, asset.decimals)}
                {#if asset.frozen}<span class="ml-1 text-xs text-blue-400">{$t('assets.frozen')}</span>{/if}
              </td>
              <td class="px-3 py-3 text-right font-mono text-xs text-gray-300">{portfolioAsset(asset.id)?.priced ? formatUsd(portfolioAsset(asset.id)?.value_usd) : '—'}</td>
              <td class="px-3 py-3 text-right font-mono text-xs {portfolioAsset(asset.id)?.priced ? 'text-gray-300' : 'text-gray-600'}">{portfolioAsset(asset.id)?.priced ? formatUsd(portfolioAsset(asset.id)?.price_usd) : $t('assets.unpriced')}</td>
              <td class="px-3 py-3 text-right font-mono text-xs text-gray-500">{portfolioAsset(asset.id)?.priced ? $t('assets.dexPriced') : '—'}</td>
              <td class="px-3 py-3 text-right">
                <span class="inline-block rounded-full px-2 py-0.5 text-xs {policyColor(asset.policy)}">
                  {$t('assets.policy_' + asset.policy)}
                </span>
              </td>
              <td class="px-3 py-3 text-right">
                <div class="flex justify-end gap-2">
                <button
                  class="rounded-lg border border-gray-700 px-3 py-1.5 text-xs text-algo-400 hover:border-algo-500/50 hover:bg-algo-500/5 disabled:cursor-not-allowed disabled:opacity-40"
                  on:click={() => openSend(asset)}
                  disabled={asset.amount <= 0 || asset.frozen}
                >
                  {$t('transfer.send')}
                </button>
                <button
                  class="rounded-lg border border-gray-700 px-2 py-1.5 text-gray-400 hover:border-gray-500 hover:text-gray-200"
                  on:click={() => openMetadataEditor(asset)}
                  title={$t('assets.editMetadata')}
                  aria-label={$t('assets.editMetadata')}
                >
                  <svg class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="1.7" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" d="m16.862 4.487 1.687-1.688a1.875 1.875 0 1 1 2.652 2.652L10.582 16.07a4.5 4.5 0 0 1-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 0 1 1.13-1.897l8.932-8.931Z" />
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 7.125 16.875 4.5" />
                  </svg>
                </button>
                </div>
              </td>
            </tr>
          {/each}

          <!-- Tier 3: Remaining ASAs -->
          {#each displayAssets.rest as asset (asset.id)}
            <tr class="hover:bg-gray-800/30">
              <td class="px-3 py-3 text-center">
                <button
                  class="text-gray-600 hover:text-gray-400"
                  on:click={() => toggleAssetPin(asset)}
                  title={$t('assets.pin')}
                >
                  <svg class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M11.48 3.499a.562.562 0 011.04 0l2.125 5.111a.563.563 0 00.475.345l5.518.442c.499.04.701.663.321.988l-4.204 3.602a.563.563 0 00-.182.557l1.285 5.385a.562.562 0 01-.84.61l-4.725-2.885a.563.563 0 00-.586 0L6.982 20.54a.562.562 0 01-.84-.61l1.285-5.386a.562.562 0 00-.182-.557l-4.204-3.602a.563.563 0 01.321-.988l5.518-.442a.563.563 0 00.475-.345L11.48 3.5z" />
                  </svg>
                </button>
              </td>
              <td class="px-3 py-3">
                <div class="flex items-center gap-2">
                  <div class="flex h-8 w-8 items-center justify-center rounded-full bg-gray-700/50">
                    <span class="text-xs font-bold text-gray-400">{(asset.unit || asset.name || '?').charAt(0).toUpperCase()}</span>
                  </div>
                  <div>
                    <p class="text-sm font-medium text-gray-200">{asset.name || $t('assets.unknown')}</p>
                    <p class="text-xs text-gray-500">{asset.unit}</p>
                    {#if assetMeta(asset.id).tag || assetMeta(asset.id).memo || assetMeta(asset.id).color_label}
                      <div class="mt-1 flex max-w-xs flex-wrap items-center gap-1">
                        {#if assetMeta(asset.id).color_label}
                          <span class="h-2 w-2 rounded-full {colorDotClass(assetMeta(asset.id).color_label)}"></span>
                        {/if}
                        {#if assetMeta(asset.id).tag}
                          <span class="rounded border border-gray-700 px-1.5 py-0.5 text-[11px] text-gray-300">{assetMeta(asset.id).tag}</span>
                        {/if}
                        {#if assetMeta(asset.id).memo}
                          <span class="max-w-[16rem] truncate text-[11px] text-gray-500">{assetMeta(asset.id).memo}</span>
                        {/if}
                      </div>
                    {/if}
                  </div>
                </div>
              </td>
              <td class="px-3 py-3 font-mono text-xs text-gray-500">{asset.id}</td>
              <td class="px-3 py-3 text-right font-mono text-sm text-gray-300">
                {formatAmount(asset.amount, asset.decimals)}
                {#if asset.frozen}<span class="ml-1 text-xs text-blue-400">{$t('assets.frozen')}</span>{/if}
              </td>
              <td class="px-3 py-3 text-right font-mono text-xs text-gray-300">{portfolioAsset(asset.id)?.priced ? formatUsd(portfolioAsset(asset.id)?.value_usd) : '—'}</td>
              <td class="px-3 py-3 text-right font-mono text-xs {portfolioAsset(asset.id)?.priced ? 'text-gray-300' : 'text-gray-600'}">{portfolioAsset(asset.id)?.priced ? formatUsd(portfolioAsset(asset.id)?.price_usd) : $t('assets.unpriced')}</td>
              <td class="px-3 py-3 text-right font-mono text-xs text-gray-500">{portfolioAsset(asset.id)?.priced ? $t('assets.dexPriced') : '—'}</td>
              <td class="px-3 py-3 text-right">
                <span class="inline-block rounded-full px-2 py-0.5 text-xs {policyColor(asset.policy)}">
                  {$t('assets.policy_' + asset.policy)}
                </span>
              </td>
              <td class="px-3 py-3 text-right">
                <div class="flex justify-end gap-2">
                <button
                  class="rounded-lg border border-gray-700 px-3 py-1.5 text-xs text-algo-400 hover:border-algo-500/50 hover:bg-algo-500/5 disabled:cursor-not-allowed disabled:opacity-40"
                  on:click={() => openSend(asset)}
                  disabled={asset.amount <= 0 || asset.frozen}
                >
                  {$t('transfer.send')}
                </button>
                <button
                  class="rounded-lg border border-gray-700 px-2 py-1.5 text-gray-400 hover:border-gray-500 hover:text-gray-200"
                  on:click={() => openMetadataEditor(asset)}
                  title={$t('assets.editMetadata')}
                  aria-label={$t('assets.editMetadata')}
                >
                  <svg class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="1.7" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" d="m16.862 4.487 1.687-1.688a1.875 1.875 0 1 1 2.652 2.652L10.582 16.07a4.5 4.5 0 0 1-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 0 1 1.13-1.897l8.932-8.931Z" />
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 7.125 16.875 4.5" />
                  </svg>
                </button>
                </div>
              </td>
            </tr>
          {/each}

          <!-- Empty state: no ASAs at all -->
          {#if !displayAssets.algo && displayAssets.pinned.length === 0 && displayAssets.rest.length === 0}
            <tr>
              <td colspan="9" class="px-3 py-8 text-center text-sm text-gray-500">
                {$t('assets.noAssets')}
              </td>
            </tr>
          {/if}
        </tbody>
      </table>
    </div>

    <!-- Advanced Indexer analysis -->
    {#if analyticsAssetOptions.length > 0}
      <div class="mt-6 rounded-lg border border-gray-800 bg-gray-900/50 p-4">
        <div class="mb-4 flex flex-wrap items-end gap-3">
          <div>
            <label class="mb-1 block text-xs text-gray-500" for="analyticsAsset">{$t('assets.analysisAsset')}</label>
            <select id="analyticsAsset" bind:value={analyticsAssetId} class="max-w-[14rem] rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none">
              {#each analyticsAssetOptions as asset}
                <option value={String(asset.id)}>{asset.unit || asset.name || asset.id}</option>
              {/each}
            </select>
          </div>
          <div>
            <label class="mb-1 block text-xs text-gray-500" for="analyticsMinRound">{$t('assets.analysisMinRound')}</label>
            <input id="analyticsMinRound" inputmode="numeric" bind:value={analyticsMinRound} class="w-32 rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none" />
          </div>
          <div>
            <label class="mb-1 block text-xs text-gray-500" for="analyticsMaxRound">{$t('assets.analysisMaxRound')}</label>
            <input id="analyticsMaxRound" inputmode="numeric" bind:value={analyticsMaxRound} class="w-32 rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none" />
          </div>
          <div>
            <label class="mb-1 block text-xs text-gray-500" for="analyticsPolicy">{$t('assets.filterPolicy')}</label>
            <select id="analyticsPolicy" bind:value={analyticsPolicy} class="rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none">
              <option value="all">{$t('assets.filterAll')}</option>
              <option value="open">{$t('assets.policy_open')}</option>
              <option value="bridged">{$t('assets.policy_bridged')}</option>
              <option value="regulated">{$t('assets.policy_regulated')}</option>
            </select>
          </div>
          <button class="btn-secondary px-3 py-1.5 text-xs" on:click={loadAssetAnalytics} disabled={analyticsLoading || !analyticsAssetId}>
            {analyticsLoading ? $t('common.loading') : $t('assets.runAnalysis')}
          </button>
        </div>
        {#if analyticsError}
          <p class="text-sm text-red-400">{analyticsError}</p>
        {:else if assetAnalytics}
          <div class="grid gap-3 md:grid-cols-3">
            <div class="rounded-md border border-gray-800 bg-gray-950/40 px-3 py-2">
              <p class="text-xs text-gray-500">{$t('assets.analysisVolume')}</p>
              <p class="mt-1 font-mono text-sm text-gray-200">{(assetAnalytics.transactions.volume || 0).toLocaleString()}</p>
              <p class="mt-1 text-xs text-gray-600">{assetAnalytics.transactions.context?.sources?.join?.(', ') || assetAnalytics.transactions.context?.source}</p>
            </div>
            <div class="rounded-md border border-gray-800 bg-gray-950/40 px-3 py-2">
              <p class="text-xs text-gray-500">{$t('assets.analysisHolders')}</p>
              <p class="mt-1 font-mono text-sm text-gray-200">{assetAnalytics.holders.total_returned}</p>
              <p class="mt-1 font-mono text-xs text-gray-600">{assetAnalytics.holders.holders?.[0]?.address ? shorten(assetAnalytics.holders.holders[0].address) : '—'}</p>
            </div>
            <div class="rounded-md border border-gray-800 bg-gray-950/40 px-3 py-2">
              <p class="text-xs text-gray-500">{$t('assets.analysisApplications')}</p>
              <p class="mt-1 font-mono text-sm text-gray-200">{assetAnalytics.applications.applications?.length || 0}</p>
              <p class="mt-1 text-xs text-gray-600">
                {$t('assets.analysisLag', { rounds: assetAnalytics.transactions.context?.rounds_behind ?? 0 })}
              </p>
            </div>
          </div>
          <div class="mt-4 overflow-x-auto rounded border border-gray-800">
            <table class="w-full text-xs">
              <thead class="bg-gray-800/50 text-gray-500">
                <tr>
                  <th class="px-2 py-2 text-left">{$t('assets.txColTime')}</th>
                  <th class="px-2 py-2 text-left">{$t('assets.txColType')}</th>
                  <th class="px-2 py-2 text-right">{$t('assets.txColAmount')}</th>
                  <th class="px-2 py-2 text-right">{$t('dashboard.lastRound')}</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-gray-800/50">
                {#each assetAnalytics.transactions.transactions || [] as tx (tx.txid)}
                  <tr>
                    <td class="px-2 py-2 text-gray-400">{formatTime(tx.timestamp)}</td>
                    <td class="px-2 py-2 text-gray-300">{txTypeLabel(tx.tx_type)}</td>
                    <td class="px-2 py-2 text-right font-mono text-gray-300">{tx.amount ? tx.amount.toLocaleString() : '—'}</td>
                    <td class="px-2 py-2 text-right font-mono text-gray-500">{tx.round}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </div>
    {/if}

    <!-- Transaction History (requires indexer) -->
    <div class="mt-6">
      <button
        class="flex w-full items-center justify-between rounded-lg border border-gray-800 bg-gray-800/30 px-4 py-3 text-sm font-medium text-gray-300 hover:bg-gray-800/50"
        on:click={toggleTxHistory}
        disabled={indexerAvailable === false}
      >
        <span class="flex items-center gap-2">
          <svg class="h-4 w-4 text-gray-400" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          {$t('assets.txHistory')}
        </span>
        <span class="flex items-center gap-2">
          {#if indexerAvailable === false}
            <span class="text-xs text-gray-600">{$t('dashboard.indexerUnavailable')}</span>
          {:else if indexerAvailable}
            <span class="flex items-center gap-1 text-xs text-green-400">
              <span class="h-1.5 w-1.5 rounded-full bg-green-400"></span>
              Indexer
            </span>
          {/if}
          <svg class="h-4 w-4 text-gray-500 transition-transform {showTxHistory ? 'rotate-180' : ''}" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
          </svg>
        </span>
      </button>

      {#if showTxHistory}
        <div class="mt-2 rounded-lg border border-gray-800 bg-gray-900/50 p-4">
          <div class="mb-4 flex flex-wrap items-end gap-3">
            <div>
              <label class="mb-1 block text-xs text-gray-500" for="txFromDate">{$t('assets.txFilterFrom')}</label>
              <input id="txFromDate" type="date" bind:value={txFromDate} class="rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none" />
            </div>
            <div>
              <label class="mb-1 block text-xs text-gray-500" for="txToDate">{$t('assets.txFilterTo')}</label>
              <input id="txToDate" type="date" bind:value={txToDate} class="rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none" />
            </div>
            <div>
              <label class="mb-1 block text-xs text-gray-500" for="txTypeFilter">{$t('assets.txFilterType')}</label>
              <select id="txTypeFilter" bind:value={txTypeFilter} class="rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none">
                <option value="all">{$t('assets.filterAll')}</option>
                <option value="pay">{$t('assets.txTypePay')}</option>
                <option value="axfer">{$t('assets.txTypeAxfer')}</option>
                <option value="afrz">{$t('assets.txTypeAfrz')}</option>
                <option value="keyreg">{$t('assets.txTypeKeyreg')}</option>
                <option value="acfg">{$t('assets.txTypeAcfg')}</option>
                <option value="appl">{$t('assets.txTypeAppl')}</option>
              </select>
            </div>
            <div>
              <label class="mb-1 block text-xs text-gray-500" for="txAssetFilter">{$t('assets.txFilterAsset')}</label>
              <select id="txAssetFilter" bind:value={txAssetFilter} class="max-w-[12rem] rounded-md border border-gray-700 bg-gray-800 px-2 py-1 text-xs text-gray-300 focus:border-algo-500 focus:outline-none">
                <option value="all">{$t('assets.filterAll')}</option>
                {#each txAssetOptions as asset}
                  <option value={asset.id}>{asset.unit || asset.name || asset.id}</option>
                {/each}
              </select>
            </div>
            <button class="btn-secondary px-3 py-1.5 text-xs" on:click={applyTxFilters} disabled={txHistoryLoading}>
              {$t('assets.txApplyFilters')}
            </button>
            <button class="btn-secondary px-3 py-1.5 text-xs" on:click={exportTxCsv} disabled={txExporting || txHistoryLoading}>
              {txExporting ? $t('common.loading') : $t('assets.txExportCsv')}
            </button>
          </div>

          {#if indexerAvailable === false}
            <div class="flex items-center gap-2 py-4 text-center">
              <svg class="mx-auto h-8 w-8 text-gray-600" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" d="M11.25 11.25l.041-.02a.75.75 0 011.063.852l-.708 2.836a.75.75 0 001.063.853l.041-.021M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9-3.75h.008v.008H12V8.25z" />
              </svg>
              <p class="w-full text-sm text-gray-500">{$t('assets.txHistoryRequiresIndexer')}</p>
            </div>
          {:else if txHistoryLoading}
            <div class="flex items-center justify-center py-6">
              <div class="animate-pulse text-algo-500">
                <svg class="h-6 w-6" viewBox="0 0 100 100" fill="none" stroke="currentColor" stroke-width="4">
                  <circle cx="50" cy="50" r="40" stroke-dasharray="60" stroke-linecap="round" />
                </svg>
              </div>
            </div>
          {:else if txHistoryError}
            <p class="py-4 text-center text-sm text-red-400">{txHistoryError}</p>
          {:else if txHistory.length > 0}
            <div class="overflow-x-auto">
              <table class="w-full text-sm">
                <thead>
                  <tr class="border-b border-gray-800 text-xs uppercase tracking-wide text-gray-500">
                    <th class="px-2 py-2 text-left">{$t('assets.txColTime')}</th>
                    <th class="px-2 py-2 text-left">{$t('assets.txColType')}</th>
                    <th class="px-2 py-2 text-right">{$t('assets.txColAmount')}</th>
                    <th class="px-2 py-2 text-left">{$t('assets.txColFrom')}</th>
                    <th class="px-2 py-2 text-left">{$t('assets.txColTo')}</th>
                    <th class="px-2 py-2 text-left">{$t('assets.txColTxid')}</th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-gray-800/50">
                  {#each txHistory as tx (tx.txid)}
                    <tr class="hover:bg-gray-800/30">
                      <td class="whitespace-nowrap px-2 py-2 text-xs text-gray-400">{formatTime(tx.timestamp)}</td>
                      <td class="px-2 py-2">
                        <span class="rounded-full bg-gray-700/50 px-2 py-0.5 text-xs text-gray-300">{txTypeLabel(tx.tx_type)}</span>
                      </td>
                      <td class="px-2 py-2 text-right font-mono text-xs text-gray-300">
                        {tx.amount > 0 ? tx.amount.toLocaleString() : '—'}
                      </td>
                      <td class="px-2 py-2 font-mono text-xs text-gray-500">{shorten(tx.sender)}</td>
                      <td class="px-2 py-2 font-mono text-xs text-gray-500">{tx.receiver ? shorten(tx.receiver) : '—'}</td>
                      <td class="px-2 py-2 font-mono text-xs text-gray-600">{shortenTxid(tx.txid)}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
            <div class="mt-3 flex flex-wrap items-center justify-between gap-2 text-xs text-gray-500">
              <span>{$t('assets.txPageSummary', { from: txOffset + 1, to: Math.min(txOffset + txLimit, txHistoryTotal), total: txHistoryTotal })}</span>
              <div class="flex gap-2">
                <button class="rounded border border-gray-700 px-2 py-1 text-gray-300 disabled:cursor-not-allowed disabled:opacity-40" on:click={previousTxPage} disabled={txOffset === 0 || txHistoryLoading}>
                  {$t('common.back')}
                </button>
                <button class="rounded border border-gray-700 px-2 py-1 text-gray-300 disabled:cursor-not-allowed disabled:opacity-40" on:click={nextTxPage} disabled={txOffset + txLimit >= txHistoryTotal || txHistoryLoading}>
                  {$t('common.continue')}
                </button>
              </div>
            </div>
          {:else}
            <p class="py-4 text-center text-sm text-gray-500">{$t('assets.txHistoryEmpty')}</p>
          {/if}

          <div class="mt-5 border-t border-gray-800 pt-4">
            <div class="mb-3 flex items-center justify-between gap-2">
              <h3 class="text-sm font-medium text-gray-300">{$t('assets.balanceSnapshots')}</h3>
              <button class="rounded border border-gray-700 px-2 py-1 text-xs text-gray-300 hover:border-gray-500" on:click={loadBalanceSnapshots} disabled={snapshotsLoading}>
                {$t('common.refresh')}
              </button>
            </div>
            {#if snapshotsLoading}
              <p class="py-3 text-sm text-gray-500">{$t('common.loading')}</p>
            {:else if snapshotsError}
              <p class="py-3 text-sm text-red-400">{snapshotsError}</p>
            {:else if balanceSnapshots.length > 0}
              <div class="max-h-64 overflow-auto rounded border border-gray-800">
                <table class="w-full text-xs">
                  <thead class="bg-gray-800/50 text-gray-500">
                    <tr>
                      <th class="px-2 py-2 text-left">{$t('assets.snapshotMonth')}</th>
                      <th class="px-2 py-2 text-left">{$t('assets.txColAsset')}</th>
                      <th class="px-2 py-2 text-right">{$t('assets.colBalance')}</th>
                      <th class="px-2 py-2 text-right">{$t('dashboard.lastRound')}</th>
                    </tr>
                  </thead>
                  <tbody class="divide-y divide-gray-800/50">
                    {#each balanceSnapshots as snapshot (`${snapshot.snapshot_month}-${snapshot.asset_id}`)}
                      <tr>
                        <td class="px-2 py-2 text-gray-400">{snapshot.snapshot_month}</td>
                        <td class="px-2 py-2 text-gray-300">{snapshot.unit || snapshot.name || snapshot.asset_id}</td>
                        <td class="px-2 py-2 text-right font-mono text-gray-300">{formatAmount(snapshot.amount, snapshot.decimals)}</td>
                        <td class="px-2 py-2 text-right font-mono text-gray-500">{snapshot.source_round}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
            {:else}
              <p class="py-3 text-sm text-gray-500">{$t('assets.balanceSnapshotsEmpty')}</p>
            {/if}
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<!-- Policy info overlay (fixed-position, escapes table overflow) -->
{#if showPolicyInfo}
  <button
    type="button"
    class="fixed inset-0 z-40 cursor-default"
    on:click={() => (showPolicyInfo = false)}
    aria-label={$t('common.cancel')}
  ></button>
  <div
    class="fixed z-50 w-72 rounded-lg border border-gray-700 bg-gray-900 px-4 py-3 text-xs leading-relaxed text-gray-300 shadow-xl"
    style="left: {policyInfoPos.x}px; top: {policyInfoPos.y}px;"
  >
    <p class="mb-2 font-medium text-gray-200">{$t('assets.colPolicy')}</p>
    <p class="text-gray-400">{$t('assets.policyTooltip')}</p>
  </div>
{/if}

{#if metadataEditAsset}
  <button
    type="button"
    class="fixed inset-0 z-40 cursor-default bg-black/60"
    on:click={closeMetadataEditor}
    aria-label={$t('common.cancel')}
  ></button>
  <div class="fixed inset-0 z-50 flex items-center justify-center px-4 py-6">
    <form
      class="w-full max-w-lg rounded-lg border border-gray-700 bg-gray-900 p-5 shadow-xl"
      on:submit|preventDefault={saveMetadataForm}
    >
      <div class="mb-4 flex items-start justify-between gap-3">
        <div>
          <h3 class="text-base font-semibold text-gray-100">{$t('assets.metadataTitle')}</h3>
          <p class="mt-1 text-xs text-gray-500">
            {metadataEditAsset.name || $t('assets.unknown')} · {metadataEditAsset.id === 0 ? $t('assets.native') : metadataEditAsset.id}
          </p>
        </div>
        <button type="button" class="rounded p-1 text-gray-500 hover:text-gray-300" on:click={closeMetadataEditor} aria-label={$t('common.cancel')}>
          <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18 18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div class="space-y-4">
        <div>
          <label class="label" for="assetTag">{$t('assets.metadataTag')}</label>
          <input
            id="assetTag"
            class="input"
            maxlength="64"
            bind:value={metadataForm.tag}
            placeholder={$t('assets.metadataTagPlaceholder')}
          />
        </div>

        <div>
          <label class="label" for="assetMemo">{$t('assets.metadataMemo')}</label>
          <textarea
            id="assetMemo"
            class="input min-h-[96px] resize-y"
            maxlength="1024"
            bind:value={metadataForm.memo}
            placeholder={$t('assets.metadataMemoPlaceholder')}
          ></textarea>
        </div>

        <div>
          <span class="label">{$t('assets.metadataColor')}</span>
          <div class="flex flex-wrap gap-2">
            {#each ASSET_COLOR_LABELS as color}
              <button
                type="button"
                class="flex h-8 min-w-8 items-center justify-center rounded-md border px-2 text-xs {metadataForm.color_label === color ? 'border-algo-400 bg-algo-500/10 text-algo-200' : 'border-gray-700 text-gray-400 hover:border-gray-500'}"
                on:click={() => (metadataForm.color_label = color)}
                aria-label={color ? $t('assets.metadataColorNamed', { color }) : $t('assets.metadataColorNone')}
                title={color ? $t('assets.metadataColorNamed', { color }) : $t('assets.metadataColorNone')}
              >
                {#if color}
                  <span class="h-3 w-3 rounded-full {colorDotClass(color)}"></span>
                {:else}
                  {$t('assets.metadataColorNoneShort')}
                {/if}
              </button>
            {/each}
          </div>
        </div>

        {#if metadataEditAsset.kind === 'asa'}
          <label class="flex cursor-pointer items-center gap-2 text-sm text-gray-300">
            <input type="checkbox" bind:checked={metadataForm.pinned} class="rounded border-gray-600 bg-gray-800 text-algo-500 focus:ring-algo-500" />
            {$t('assets.metadataPinned')}
          </label>
        {/if}
      </div>

      {#if metadataError}
        <p class="mt-4 text-sm text-red-400">{metadataError}</p>
      {/if}

      <div class="mt-6 flex flex-wrap justify-between gap-2">
        <button
          type="button"
          class="rounded-lg border border-red-900/60 px-3 py-2 text-sm text-red-300 hover:bg-red-950/40 disabled:cursor-not-allowed disabled:opacity-50"
          on:click={clearMetadataForm}
          disabled={metadataSaving}
        >
          {$t('assets.metadataClear')}
        </button>
        <div class="flex gap-2">
          <button type="button" class="btn-secondary px-4 py-2 text-sm" on:click={closeMetadataEditor} disabled={metadataSaving}>
            {$t('common.cancel')}
          </button>
          <button type="submit" class="btn-primary px-4 py-2 text-sm" disabled={metadataSaving}>
            {metadataSaving ? $t('common.loading') : $t('common.save')}
          </button>
        </div>
      </div>
    </form>
  </div>
{/if}

{#if sendAsset && currentWallet && accountInfo}
  <TransferModal
    wallet={currentWallet}
    asset={sendAsset}
    {accountInfo}
    algoAsset={displayAssets.algo}
    on:close={() => (sendAsset = null)}
    on:completed={handleTransferCompleted}
  />
{/if}

{#if showOptInModal && currentWallet}
  <OptInModal
    wallet={currentWallet}
    on:close={() => (showOptInModal = false)}
    on:completed={handleTransferCompleted}
  />
{/if}

{#if showReceiveModal && currentWallet}
  <ReceiveModal wallet={currentWallet} on:close={() => (showReceiveModal = false)} />
{/if}
