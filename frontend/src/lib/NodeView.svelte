<script>
  import { onMount, onDestroy } from 'svelte';
  import { api } from '../api.js';
  import { t } from '../i18n/index.js';

  export let nodeStatus = null;
  export let syncProgress = null;
  export let loading = true;
  export let error = '';
  export let loadNodeStatus;

  // Indexer sync state
  let indexerStatus = null;
  let indexerProgress = null;
  let indexerTimer = null;

  // Block info list and participation stats
  let blockInfoList = [];
  let participationStats = null;

  function formatRound(round) {
    if (round === null || round === undefined) return '—';
    return Number(round).toLocaleString();
  }

  function formatTime(ns) {
    if (!ns) return '—';
    const secs = Math.round(ns / 1_000_000_000);
    return `${secs}s`;
  }

  function protocolShort(version) {
    if (!version) return '—';
    const match = version.match(/\/([0-9a-f]{8,})/);
    return match ? match[1].slice(0, 8) : version.slice(0, 20);
  }

  function formatEta(seconds) {
    if (seconds == null) return '—';
    if (seconds === 0) return $t('dashboard.synced');
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
    const hours = Math.floor(seconds / 3600);
    const mins = Math.round((seconds % 3600) / 60);
    return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`;
  }

  function formatSpeed(bps) {
    if (bps == null) return '—';
    if (bps >= 1000) return `${(bps / 1000).toFixed(1)}k`;
    return bps.toFixed(1);
  }

  function formatBlockTime(unixSeconds) {
    if (!unixSeconds) return '—';
    const date = new Date(unixSeconds * 1000);
    return date.toLocaleTimeString();
  }

  function shortAddress(addr) {
    if (!addr) return '—';
    if (addr.length <= 12) return addr;
    return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
  }

  // Determine sync state from the actual sync progress (local round vs
  // network round) rather than algod's self-reported `catchup_time`, which
  // can disagree with the real network round. Fall back to `catchup_time`
  // only when the network round is unknown.
  $: isSynced =
    syncProgress && syncProgress.network_round != null
      ? syncProgress.rounds_behind === 0
      : nodeStatus?.catchup_time === 0;
  // Always show the node sync progress card while we have a network round,
  // even after catch-up completes, so the panel stays stable instead of
  // appearing and disappearing.
  $: showProgress = syncProgress && syncProgress.network_round != null;

  // Fetch indexer status + progress, block info, and participation stats on
  // mount and poll every 10s.
  onMount(async () => {
    await refreshAll();
    indexerTimer = setInterval(refreshAll, 10000);
  });

  onDestroy(() => {
    if (indexerTimer) clearInterval(indexerTimer);
  });

  async function refreshAll() {
    await Promise.all([refreshIndexer(), refreshBlockAndParticipation()]);
  }

  async function refreshIndexer() {
    try {
      indexerStatus = await api.getIndexerStatus();
      if (indexerStatus?.available) {
        indexerProgress = await api.getIndexerSyncProgress();
      }
    } catch {
      // Indexer endpoints may 503 if not configured; ignore silently.
    }
  }

  async function refreshBlockAndParticipation() {
    try {
      const [info, stats] = await Promise.all([
        api.getBlockInfo(),
        api.getParticipationStats(),
      ]);
      blockInfoList = Array.isArray(info) ? info : [];
      participationStats = stats;
    } catch {
      // Block info may 503 while node is starting; ignore silently.
    }
  }

  // Always show the indexer sync progress card while the indexer is available,
  // even after catch-up completes, so the user can always see live indexing status.
  $: showIndexerProgress = indexerStatus?.available && indexerProgress;
</script>

<section>
  <div class="mb-4">
    <h2 class="text-lg font-semibold text-gray-200">{$t('dashboard.nodeStatus')}</h2>
  </div>
  {#if loading}
    <div class="flex items-center justify-center py-12">
      <div class="animate-pulse text-algo-500">
        <svg class="h-10 w-10" viewBox="0 0 100 100" fill="none" stroke="currentColor" stroke-width="4">
          <circle cx="50" cy="50" r="40" stroke-dasharray="60" stroke-linecap="round" />
        </svg>
      </div>
    </div>
  {:else if error && !nodeStatus}
    <div class="card text-center">
      <p class="text-red-400">{error}</p>
      <button class="btn-primary mt-4" on:click={loadNodeStatus}>{$t('common.retry')}</button>
    </div>
  {:else}
    <div class="grid grid-cols-2 gap-4 md:grid-cols-4">
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.lastRound')}</p>
        <p class="mt-2 font-mono text-2xl font-bold text-algo-400">
          {formatRound(nodeStatus?.last_round)}
        </p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.syncStatus')}</p>
        <p class="mt-2 text-2xl font-bold {isSynced ? 'text-green-400' : 'text-yellow-400'}">
          {isSynced ? $t('dashboard.synced') : $t('dashboard.catchingUp')}
        </p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.lastBlock')}</p>
        <p class="mt-2 font-mono text-2xl font-bold text-gray-200">
          {formatTime(nodeStatus?.time_since_last_round)}
        </p>
      </div>
      <div class="card">
        <p class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.protocol')}</p>
        <p class="mt-2 truncate font-mono text-lg font-bold text-gray-200" title={nodeStatus?.last_version}>
          {protocolShort(nodeStatus?.last_version)}
        </p>
      </div>
    </div>

    <!-- Sync progress detail (always shown when network round is known) -->
    {#if showProgress}
      <div class="card mt-4">
        <div class="mb-3 flex items-center justify-between">
          <h3 class="text-sm font-semibold text-gray-300">{$t('dashboard.syncProgress')}</h3>
          <span class="font-mono text-sm text-yellow-400">
            {syncProgress.progress_pct != null ? `${syncProgress.progress_pct.toFixed(1)}%` : '—'}
          </span>
        </div>

        <!-- Progress bar -->
        <div class="mb-4 h-2 w-full overflow-hidden rounded-full bg-gray-700">
          <div
            class="h-full rounded-full bg-gradient-to-r from-algo-600 to-algo-400 transition-all duration-1000"
            style="width: {syncProgress.progress_pct != null ? syncProgress.progress_pct : 0}%"
          ></div>
        </div>

        <!-- Detail grid -->
        <div class="grid grid-cols-2 gap-4 md:grid-cols-4">
          <div>
            <p class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.networkRound')}</p>
            <p class="mt-1 font-mono text-sm font-semibold text-gray-200">
              {formatRound(syncProgress.network_round)}
            </p>
          </div>
          <div>
            <p class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.roundsBehind')}</p>
            <p class="mt-1 font-mono text-sm font-semibold text-yellow-400">
              {formatRound(syncProgress.rounds_behind)}
            </p>
          </div>
          <div>
            <p class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.syncSpeed')}</p>
            <p class="mt-1 font-mono text-sm font-semibold text-gray-200">
              {formatSpeed(syncProgress.blocks_per_sec)} {$t('dashboard.blocksPerSec')}
            </p>
          </div>
          <div>
            <p class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.estimatedRemaining')}</p>
            <p class="mt-1 font-mono text-sm font-semibold text-algo-400">
              {formatEta(syncProgress.estimated_seconds_remaining)}
            </p>
          </div>
        </div>
      </div>
    {/if}

    <!-- Indexer sync progress (shown when indexer is available and syncing) -->
    {#if showIndexerProgress}
      <div class="card mt-4">
        <div class="mb-3 flex items-center justify-between">
          <h3 class="text-sm font-semibold text-gray-300">{$t('dashboard.indexerSyncProgress')}</h3>
          <span class="font-mono text-sm text-yellow-400">
            {indexerProgress.progress_pct != null ? `${indexerProgress.progress_pct.toFixed(1)}%` : '—'}
          </span>
        </div>

        <!-- Progress bar -->
        <div class="mb-4 h-2 w-full overflow-hidden rounded-full bg-gray-700">
          <div
            class="h-full rounded-full bg-gradient-to-r from-blue-600 to-blue-400 transition-all duration-1000"
            style="width: {indexerProgress.progress_pct != null ? indexerProgress.progress_pct : 0}%"
          ></div>
        </div>

        <!-- Detail grid -->
        <div class="grid grid-cols-2 gap-4 md:grid-cols-4">
          <div>
            <p class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.indexedRound')}</p>
            <p class="mt-1 font-mono text-sm font-semibold text-gray-200">
              {formatRound(indexerProgress.indexed_round)}
            </p>
          </div>
          <div>
            <p class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.roundsBehind')}</p>
            <p class="mt-1 font-mono text-sm font-semibold text-yellow-400">
              {formatRound(indexerProgress.rounds_behind)}
            </p>
          </div>
          <div>
            <p class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.syncSpeed')}</p>
            <p class="mt-1 font-mono text-sm font-semibold text-gray-200">
              {formatSpeed(indexerProgress.blocks_per_sec)} {$t('dashboard.blocksPerSec')}
            </p>
          </div>
          <div>
            <p class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.estimatedRemaining')}</p>
            <p class="mt-1 font-mono text-sm font-semibold text-blue-400">
              {formatEta(indexerProgress.estimated_seconds_remaining)}
            </p>
          </div>
        </div>
      </div>
    {/if}

    <!-- Latest blocks list + Node participation (two-column) -->
    {#if blockInfoList.length > 0 || participationStats}
      <div class="mt-4 grid grid-cols-1 gap-4 md:grid-cols-2">
        <!-- Recent Blocks -->
        {#if blockInfoList.length > 0}
          <div class="card flex flex-col">
            <h3 class="mb-3 text-sm font-semibold text-gray-300">{$t('dashboard.recentBlocks')}</h3>
            <div class="max-h-80 space-y-1 overflow-y-auto pr-1">
              {#each blockInfoList as block}
                <div class="flex items-center justify-between rounded-md px-2 py-1.5 hover:bg-gray-800/50">
                  <div class="flex items-center gap-3">
                    <span class="font-mono text-xs font-semibold text-algo-400">{formatRound(block.round)}</span>
                    <span class="font-mono text-xs text-gray-400">{formatBlockTime(block.timestamp)}</span>
                  </div>
                  <div class="flex items-center gap-3">
                    <span class="font-mono text-xs text-gray-300">{block.txn_count} {$t('dashboard.txns')}</span>
                    <span class="font-mono text-xs text-gray-500" title={block.proposer}>
                      {shortAddress(block.proposer)}
                    </span>
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <!-- Node Participation -->
        {#if participationStats}
          <div class="card">
            <h3 class="mb-3 text-sm font-semibold text-gray-300">{$t('dashboard.participation')}</h3>
            {#if participationStats.participating}
              <div class="space-y-2.5">
                <div class="flex items-center justify-between">
                  <span class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.blocksProposed')}</span>
                  <span class="font-mono text-lg font-bold text-green-400">
                    {formatRound(participationStats.blocks_proposed)}
                  </span>
                </div>
                <div class="flex items-center justify-between">
                  <span class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.blocksScanned')}</span>
                  <span class="font-mono text-sm text-gray-200">
                    {formatRound(participationStats.blocks_scanned)}
                  </span>
                </div>
                <div class="flex items-center justify-between">
                  <span class="text-xs uppercase tracking-wider text-gray-500">{$t('dashboard.participationRate')}</span>
                  <span class="font-mono text-sm text-gray-200">
                    {participationStats.blocks_scanned > 0
                      ? `${((participationStats.blocks_proposed / participationStats.blocks_scanned) * 100).toFixed(2)}%`
                      : '—'}
                  </span>
                </div>
              </div>
            {:else}
              <div class="flex flex-col items-center justify-center py-6 text-center">
                <svg class="mb-2 h-8 w-8 text-gray-600" fill="none" stroke="currentColor" stroke-width="1.5" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M11.42 15.17 17.25 21A2.652 2.652 0 0 0 21 17.25l-5.877-5.877M11.42 15.17l2.496-3.03c.317-.384.74-.626 1.208-.766M11.42 15.17l-4.655 5.653a2.548 2.548 0 1 1-3.586-3.586l6.837-5.63m5.108-.233c.55-.164 1.163-.188 1.743-.14a4.5 4.5 0 0 0 4.486-6.336l-3.276 3.277a3.004 3.004 0 0 1-2.25-2.25l3.276-3.276a4.5 4.5 0 0 0-6.336 4.486c.091 1.076-.071 2.264-.904 2.95l-.102.085m-1.745 1.437L5.909 7.5H4.5L2.25 3.75l1.5-1.5L7.5 4.5v1.409l4.26 4.26m-1.745 1.437 1.745-1.437m6.615 8.206L15.75 15.75M4.867 19.125h.008v.008h-.008v-.008Z" />
                </svg>
                <p class="text-sm text-gray-400">{$t('dashboard.notParticipating')}</p>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</section>
