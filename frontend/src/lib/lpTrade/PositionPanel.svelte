<script>
  import { t } from '../../i18n/index.js';

  export let positions = [];
  export let positionsNote = '';
  export let loadingPositions = false;
  export let creator = '';
  export let assetLabel = (assetId) => `#${assetId}`;
  export let formatAsset = (raw) => String(raw ?? '0');
  export let formatLp = (raw) => String(raw ?? '0');
  export let formatSharePpm = (ppm) => String(ppm ?? '0');
  export let onRefresh = () => {};
  export let onSelectPool = () => {};
</script>

<section class="card">
  <div class="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
    <div>
      <h3 class="text-lg font-semibold text-gray-100">{$t('lpTrade.myLiquidity')}</h3>
      <p class="mt-2 max-w-2xl text-sm text-gray-500">{$t('lpTrade.myLiquidityHint')}</p>
    </div>
    <button class="btn-secondary shrink-0" type="button" on:click={onRefresh} disabled={loadingPositions || !creator}>
      {loadingPositions ? $t('common.loading') : $t('lpTrade.refreshPositions')}
    </button>
  </div>

  {#if positionsNote}
    <p class="mt-3 text-sm text-yellow-300">{positionsNote}</p>
  {/if}

  <div class="mt-4 space-y-3">
    {#if positions.length === 0 && !loadingPositions}
      <p class="text-sm text-gray-500">{$t('lpTrade.noPositions')}</p>
    {:else}
      {#each positions as item (`${item.pool.source}:${item.pool.pool_id || item.pool.app_id}`)}
        <button
          class="w-full rounded-lg border border-gray-700 bg-surface-dark p-3 text-left transition hover:border-algo-500/50"
          type="button"
          on:click={() => onSelectPool(item.pool)}
        >
          <div class="flex flex-wrap items-center justify-between gap-2">
            <span class="font-mono text-sm text-gray-100">
              {item.pool.source} · {item.pool.source === 'tinyman' ? item.pool.pool_id : `App ${item.pool.app_id}`} · {assetLabel(item.pool.asset_0)} / {assetLabel(item.pool.asset_1)}
            </span>
            <span class="rounded-full bg-algo-500/10 px-2 py-0.5 text-xs text-algo-300">{formatSharePpm(item.pool_share_ppm)}</span>
          </div>
          <div class="mt-2 grid gap-2 text-xs text-gray-400 sm:grid-cols-3">
            <div>{$t('lpTrade.lpBalance')}: <span class="font-mono text-gray-200">{formatLp(item.lp_balance)} {assetLabel(item.lp_asset_id)}</span></div>
            <div>{$t('lpTrade.underlyingAssets')}: <span class="font-mono text-gray-200">{formatAsset(item.underlying_0, item.pool.asset_0)} / {formatAsset(item.underlying_1, item.pool.asset_1)}</span></div>
            <div>{$t('lpTrade.sourceRound')}: <span class="font-mono text-gray-200">{item.pool.source_round}</span></div>
          </div>
        </button>
      {/each}
    {/if}
  </div>
</section>
