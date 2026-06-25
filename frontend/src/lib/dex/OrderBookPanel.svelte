<script>
  import { t } from '../../i18n/index.js';
  import { depthWidth as defaultDepthWidth } from './viewModel.js';

  export let book = null;
  export let bookLoading = false;
  export let quoteAssetId = '';
  export let pairLabel = '';
  export let mobileTab = 'order';
  export let syntheticAsks = [];
  export let syntheticBids = [];
  export let maxAskAmount = 0;
  export let maxBidAmount = 0;
  export let maxSyntheticAskAmount = 0;
  export let maxSyntheticBidAmount = 0;
  export let fmtPrice = (value) => String(value ?? '—');
  export let fmtBaseAmount = (value) => String(value ?? '—');
  export let clickLevel = () => {};
  export let handleLevelKeydown = () => {};
  export let widthForDepth = defaultDepthWidth;
</script>

<div class="card orderbook-card p-4 lg:block {mobileTab === 'orderbook' ? 'block' : 'hidden'}">
  <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
    <div class="flex items-center gap-2">
      <h3 class="text-sm font-semibold text-gray-200">{$t('dex.orderbook')}</h3>
    </div>
    {#if pairLabel}
      <span class="font-mono text-xs text-gray-500">{pairLabel}</span>
    {/if}
  </div>

  {#if book}
    <div class="grid grid-cols-3 px-2 py-1 text-[10px] font-medium uppercase tracking-wider text-gray-500">
      <span>{$t('dex.price')}</span>
      <span class="text-right">{$t('dex.amount')}</span>
      <span class="text-right">{$t('dex.total')}</span>
    </div>

    <div class="asks-section">
      {#each [...book.asks].reverse() as level}
        <div
          class="ob-row group relative grid grid-cols-3 px-2 py-0.5 cursor-pointer font-mono text-xs text-red-300 hover:bg-red-500/10"
          role="button"
          tabindex="0"
          on:click={() => clickLevel(level)}
          on:keydown={(event) => handleLevelKeydown(event, level)}
        >
          <div
            class="absolute inset-y-0 right-0 bg-red-500/10"
            style="width: {widthForDepth(level.amount, maxAskAmount)}%"
          ></div>
          <span class="relative z-10">{fmtPrice(level.price)}</span>
          <span class="relative z-10 text-right text-gray-300">{fmtBaseAmount(level.amount)}</span>
          <span class="relative z-10 text-right text-gray-500">{fmtBaseAmount(level.total)}</span>
        </div>
      {/each}
      {#if book.asks.length === 0}
        <div class="px-2 py-2 text-center text-xs text-gray-600">{$t('dex.noAsks')}</div>
      {/if}
    </div>

    <div class="spread-row my-1 flex items-center justify-center border-y border-gray-700/50 bg-gray-800/30 py-1">
      <span class="font-mono text-xs text-gray-400">
        {$t('dex.spread')}: {fmtPrice(book.spread)}
      </span>
    </div>

    <div class="bids-section">
      {#each book.bids as level}
        <div
          class="ob-row group relative grid grid-cols-3 px-2 py-0.5 cursor-pointer font-mono text-xs text-green-300 hover:bg-green-500/10"
          role="button"
          tabindex="0"
          on:click={() => clickLevel(level)}
          on:keydown={(event) => handleLevelKeydown(event, level)}
        >
          <div
            class="absolute inset-y-0 right-0 bg-green-500/10"
            style="width: {widthForDepth(level.amount, maxBidAmount)}%"
          ></div>
          <span class="relative z-10">{fmtPrice(level.price)}</span>
          <span class="relative z-10 text-right text-gray-300">{fmtBaseAmount(level.amount)}</span>
          <span class="relative z-10 text-right text-gray-500">{fmtBaseAmount(level.total)}</span>
        </div>
      {/each}
      {#if book.bids.length === 0}
        <div class="px-2 py-2 text-center text-xs text-gray-600">{$t('dex.noBids')}</div>
      {/if}
    </div>

    {#if syntheticAsks.length > 0 || syntheticBids.length > 0}
      <div class="mt-4 border-t border-gray-800 pt-3">
        <div class="mb-2 flex items-center justify-between gap-2">
          <h4 class="text-[11px] font-semibold uppercase text-gray-500">{$t('dex.syntheticDepth')}</h4>
        </div>
        <div class="grid grid-cols-4 px-2 py-1 text-[10px] font-medium uppercase tracking-wider text-gray-500">
          <span>{$t('dex.source')}</span>
          <span class="text-right">{$t('dex.price')}</span>
          <span class="text-right">{$t('dex.amount')}</span>
          <span class="text-right">{$t('dex.impact')}</span>
        </div>
        <div class="space-y-0.5">
          {#each [...syntheticAsks].reverse() as level}
            <div class="relative grid grid-cols-4 px-2 py-1 font-mono text-[11px] text-red-200" title={level.note}>
              <div
                class="absolute inset-y-0 right-0 bg-red-500/5"
                style="width: {widthForDepth(level.amount, maxSyntheticAskAmount)}%"
              ></div>
              <span class="relative z-10 truncate text-gray-400">{level.source_label}</span>
              <span class="relative z-10 text-right">{fmtPrice(level.price)}</span>
              <span class="relative z-10 text-right text-gray-300">{fmtBaseAmount(level.amount)}</span>
              <span class="relative z-10 text-right text-gray-500">{level.price_impact_bps}bps</span>
            </div>
          {/each}
          {#each syntheticBids as level}
            <div class="relative grid grid-cols-4 px-2 py-1 font-mono text-[11px] text-green-200" title={level.note}>
              <div
                class="absolute inset-y-0 right-0 bg-green-500/5"
                style="width: {widthForDepth(level.amount, maxSyntheticBidAmount)}%"
              ></div>
              <span class="relative z-10 truncate text-gray-400">{level.source_label}</span>
              <span class="relative z-10 text-right">{fmtPrice(level.price)}</span>
              <span class="relative z-10 text-right text-gray-300">{fmtBaseAmount(level.amount)}</span>
              <span class="relative z-10 text-right text-gray-500">{level.price_impact_bps}bps</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {:else if bookLoading}
    <div class="grid grid-cols-3 px-2 py-1 text-[10px] font-medium uppercase tracking-wider text-gray-500">
      <span>{$t('dex.price')}</span>
      <span class="text-right">{$t('dex.amount')}</span>
      <span class="text-right">{$t('dex.total')}</span>
    </div>
    <div class="py-8 text-center text-xs text-gray-500">{$t('dex.loading')}</div>
  {:else if quoteAssetId}
    <div class="grid grid-cols-3 px-2 py-1 text-[10px] font-medium uppercase tracking-wider text-gray-500">
      <span>{$t('dex.price')}</span>
      <span class="text-right">{$t('dex.amount')}</span>
      <span class="text-right">{$t('dex.total')}</span>
    </div>
    <div class="py-8 text-center text-xs text-gray-600">{$t('dex.noAsks')}</div>
  {:else}
    <div class="grid grid-cols-3 px-2 py-1 text-[10px] font-medium uppercase tracking-wider text-gray-500">
      <span>{$t('dex.price')}</span>
      <span class="text-right">{$t('dex.amount')}</span>
      <span class="text-right">{$t('dex.total')}</span>
    </div>
    <div class="py-8 text-center text-xs text-gray-600">{$t('dex.selectPair')}</div>
  {/if}
</div>

<style>
  .ob-row {
    transition: background-color 0.1s ease;
  }
</style>
