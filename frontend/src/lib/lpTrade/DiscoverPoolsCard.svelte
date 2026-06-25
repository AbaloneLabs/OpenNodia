<script>
  import { t } from '../../i18n/index.js';
  import AssetSearch from '../AssetSearch.svelte';

  export let assetA = '0';
  export let assetB = '';
  export let loadingPools = false;
  export let discoveryNote = '';
  export let pools = [];
  export let selectedAssetFromInput = () => null;
  export let sourceBadgeClass = () => '';
  export let sourceStatusLabel = () => '';
  export let onDiscoverAssetASelect = () => {};
  export let onDiscoverAssetBSelect = () => {};
  export let switchDiscoverPair = () => {};
  export let discoverPools = () => {};
  export let loadPool = () => {};
  export let choosePool = () => {};
  export let assetLabel = (assetId) => `#${assetId}`;
  export let formatAsset = (raw) => String(raw ?? '0');
</script>

<div class="card">
  <h3 class="text-lg font-semibold text-gray-100">{$t('lpTrade.discoverPools')}</h3>
  <p class="mt-2 text-sm text-gray-500">{$t('lpTrade.discoverPoolsHint')}</p>
  <p class="mt-1 text-xs text-gray-500">{$t('lpTrade.noTvlVolumeHint')}</p>
  <div class="mt-3 flex flex-wrap gap-2 text-xs">
    <span class="rounded-full bg-algo-500/10 px-2 py-0.5 text-algo-300">OpenNodia Pool · {$t('dex.sourceReady')}</span>
    <span class="rounded-full px-2 py-0.5 {sourceBadgeClass('tinyman')}">Tinyman · {sourceStatusLabel('tinyman')}</span>
    <span class="rounded-full px-2 py-0.5 {sourceBadgeClass('pact')}">Pact · {sourceStatusLabel('pact')}</span>
  </div>
  <div class="mt-4 grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)]">
    <div class="block">
      <span class="label">{$t('lpTrade.assetA')}</span>
      <div class="mt-1">
        <AssetSearch
          placeholder="0 (ALGO)"
          allowManualEntry={true}
          selectedAsset={selectedAssetFromInput(assetA)}
          on:select={(event) => onDiscoverAssetASelect(event.detail)}
        />
      </div>
    </div>
    <div class="flex items-end">
      <button class="btn-secondary px-3 py-2 text-xs" type="button" on:click={switchDiscoverPair}>
        {$t('dex.swapPair')}
      </button>
    </div>
    <div class="block">
      <span class="label">{$t('lpTrade.assetB')}</span>
      <div class="mt-1">
        <AssetSearch
          placeholder="ASA ID / name"
          allowManualEntry={true}
          selectedAsset={selectedAssetFromInput(assetB)}
          on:select={(event) => onDiscoverAssetBSelect(event.detail)}
        />
      </div>
    </div>
  </div>
  <button class="btn-primary mt-4" type="button" on:click={discoverPools} disabled={loadingPools}>
    {loadingPools ? $t('lpTrade.searching') : $t('lpTrade.searchPools')}
  </button>

  {#if discoveryNote}
    <p class="mt-3 text-sm text-yellow-300">{discoveryNote}</p>
  {/if}

  <div class="mt-4 space-y-2">
    {#if pools.length === 0 && !loadingPools}
      <p class="text-sm text-gray-500">{$t('lpTrade.noPools')}</p>
    {:else}
      {#each pools as item (`${item.source}:${item.pool_id || item.app_id}`)}
        <button
          class="w-full rounded-lg border border-gray-700 bg-surface-dark p-3 text-left transition hover:border-algo-500/50"
          type="button"
          on:click={() => item.source === 'native' ? loadPool(item.app_id) : choosePool(item)}
        >
          <div class="flex items-center justify-between gap-3">
            <span class="font-mono text-sm text-gray-100">
              {item.source} · {item.source === 'tinyman' ? item.pool_id : `App ${item.app_id}`}
            </span>
            <span class="rounded-full bg-algo-500/10 px-2 py-0.5 text-xs text-algo-300">{item.fee_bps} bps</span>
          </div>
          <div class="mt-1 text-xs text-gray-500">
            {assetLabel(item.asset_0)} / {assetLabel(item.asset_1)}
          </div>
          <div class="mt-1 text-xs text-gray-400">
            {$t('lpTrade.reserves')}: <span class="font-mono text-gray-200">{formatAsset(item.reserve_0, item.asset_0)} / {formatAsset(item.reserve_1, item.asset_1)}</span>
          </div>
          <div class="mt-1 text-xs text-gray-500">
            {$t('lpTrade.sourceRound')}: {item.source_round} · {item.status || item.lifecycle}
          </div>
        </button>
      {/each}
    {/if}
  </div>
</div>
