<script>
  import { t } from '../../i18n/index.js';
  import AssetSearch from '../AssetSearch.svelte';

  export let quoteAssetMeta = null;
  export let baseAssetMeta = null;
  export let book = null;
  export let quoteAssetId = '';
  export let bookLoading = false;
  export let bestBid = null;
  export let bestAsk = null;
  export let onQuoteSelect = () => {};
  export let onBaseSelect = () => {};
  export let swapPair = () => {};
  export let fmtPrice = (value) => String(value ?? '—');
</script>

<div class="market-header card mb-4 p-4">
  <div class="flex flex-wrap items-center gap-4">
    <div class="pair-selector flex flex-1 flex-wrap items-end gap-2 sm:flex-none">
      <div class="flex min-w-0 flex-1 flex-col sm:flex-none">
        <label for="dex-quote-asset" class="text-[10px] uppercase tracking-wider text-gray-500">{$t('dex.quoteAsset')}</label>
        <div class="w-full min-w-0 sm:w-36">
          <AssetSearch
            placeholder={$t('dex.assetSearchPlaceholder')}
            allowManualEntry={true}
            selectedAsset={quoteAssetMeta}
            on:select={onQuoteSelect}
          />
        </div>
      </div>
      <button
        class="swap-btn shrink-0 rounded-md border border-gray-700 bg-gray-800 p-2 text-gray-400 hover:text-algo-400"
        on:click={swapPair}
        title={$t('dex.swapPair')}
      >
        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4" />
        </svg>
      </button>
      <div class="flex min-w-0 flex-1 flex-col sm:flex-none">
        <label for="dex-base-asset" class="text-[10px] uppercase tracking-wider text-gray-500">{$t('dex.baseAsset')}</label>
        <div class="w-full min-w-0 sm:w-36">
          <AssetSearch
            placeholder={$t('dex.algoAssetPlaceholder')}
            allowManualEntry={true}
            selectedAsset={baseAssetMeta}
            on:select={onBaseSelect}
          />
        </div>
      </div>
    </div>

    <div class="hidden h-12 w-px bg-gray-700 sm:block"></div>

    {#if book}
      <div class="flex flex-1 flex-wrap items-center gap-x-8 gap-y-2">
        <div class="stat-block">
          <div class="text-[10px] uppercase tracking-wider text-gray-500">{$t('dex.lastPrice')}</div>
          <div class="font-mono text-lg font-semibold text-gray-100">
            {book.last_price != null ? fmtPrice(book.last_price) : '—'}
          </div>
        </div>
        <div class="stat-block">
          <div class="text-[10px] uppercase tracking-wider text-gray-500">{$t('dex.spread')}</div>
          <div class="font-mono text-sm text-gray-300">{fmtPrice(book.spread)}</div>
        </div>
        <div class="stat-block">
          <div class="text-[10px] uppercase tracking-wider text-gray-500">{$t('dex.bestBid')}</div>
          <div class="font-mono text-sm text-green-400">{bestBid != null ? fmtPrice(bestBid) : '—'}</div>
        </div>
        <div class="stat-block">
          <div class="text-[10px] uppercase tracking-wider text-gray-500">{$t('dex.bestAsk')}</div>
          <div class="font-mono text-sm text-red-400">{bestAsk != null ? fmtPrice(bestAsk) : '—'}</div>
        </div>
      </div>
    {:else if quoteAssetId}
      <div class="flex-1 text-sm text-gray-500">
        {bookLoading ? $t('dex.loading') : $t('dex.selectPair')}
      </div>
    {:else}
      <div class="flex-1 text-sm text-gray-500">{$t('dex.selectPair')}</div>
    {/if}
  </div>
</div>
