<script>
  import { t } from '../../i18n/index.js';

  export let mobileTab = 'order';
  export let pairLabel = '';
  export let chartRanges = [];
  export let chartRange = 'all';
  export let sparklineData = null;
  export let sparklinePoints = '';
  export let sparklineColor = '#00d4aa';
  export let quoteAssetId = '';
  export let fmtPrice = (value) => String(value ?? '—');
</script>

<div class="card p-4 lg:block {mobileTab === 'chart' ? 'block' : 'hidden'}">
  <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
    <h3 class="text-sm font-semibold text-gray-200">{$t('dex.priceChart')}</h3>
    <div class="flex items-center gap-2">
      {#if pairLabel}
        <span class="font-mono text-xs text-gray-500">{pairLabel}</span>
      {/if}
      <div class="flex rounded-md bg-gray-800/60 p-0.5">
        {#each chartRanges as range}
          <button
            type="button"
            class="rounded px-2 py-1 text-[10px] font-medium transition {chartRange === range.id
              ? 'bg-gray-700 text-gray-100'
              : 'text-gray-500 hover:text-gray-300'}"
            on:click={() => (chartRange = range.id)}
          >
            {$t(range.labelKey)}
          </button>
        {/each}
      </div>
    </div>
  </div>

  {#if sparklineData}
    <div class="relative">
      <svg class="sparkline w-full" viewBox="0 0 100 30" preserveAspectRatio="none">
        {#if sparklineData.single}
          <circle cx="50" cy="15" r="2.2" fill={sparklineColor} vector-effect="non-scaling-stroke" />
        {:else}
          <polyline
            points={sparklinePoints}
            fill="none"
            stroke={sparklineColor}
            stroke-width="1.5"
            vector-effect="non-scaling-stroke"
          />
        {/if}
      </svg>
      <span class="absolute left-0 top-0 font-mono text-[9px] text-gray-600">{fmtPrice(sparklineData.max)}</span>
      <span class="absolute bottom-0 left-0 font-mono text-[9px] text-gray-600">{fmtPrice(sparklineData.min)}</span>
    </div>
    <p class="mt-1 text-right text-[10px] text-gray-600">{$t('dex.priceChartHint')}</p>
  {:else if quoteAssetId}
    <div class="chart-empty flex h-16 items-center justify-center text-xs text-gray-600">
      {$t('dex.noTrades')}
    </div>
  {:else}
    <div class="chart-empty flex h-16 items-center justify-center text-xs text-gray-600">
      {$t('dex.selectPair')}
    </div>
  {/if}
</div>

<style>
  .sparkline {
    height: 40px;
  }

  .chart-empty {
    border: 1px dashed theme('colors.gray.700');
    border-radius: 0.375rem;
  }
</style>
