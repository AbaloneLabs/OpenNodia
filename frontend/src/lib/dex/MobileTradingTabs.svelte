<script>
  import { t } from '../../i18n/index.js';

  export let mobileTab = 'order';
</script>

<div class="mobile-tabs mb-3 flex gap-1 rounded-lg bg-gray-800/60 p-1 lg:hidden">
  <button
    type="button"
    class="min-h-[44px] flex-1 rounded-md px-2 py-2 text-xs font-medium transition {mobileTab === 'chart'
      ? 'bg-gray-700 text-gray-100'
      : 'text-gray-400 hover:text-gray-200'}"
    on:click={() => (mobileTab = 'chart')}
  >
    {$t('dex.priceChart')}
  </button>
  <button
    type="button"
    class="min-h-[44px] flex-1 rounded-md px-2 py-2 text-xs font-medium transition {mobileTab === 'orderbook'
      ? 'bg-gray-700 text-gray-100'
      : 'text-gray-400 hover:text-gray-200'}"
    on:click={() => (mobileTab = 'orderbook')}
  >
    {$t('dex.orderbook')}
  </button>
  <button
    type="button"
    class="min-h-[44px] flex-1 rounded-md px-2 py-2 text-xs font-medium transition {mobileTab === 'order'
      ? 'bg-gray-700 text-gray-100'
      : 'text-gray-400 hover:text-gray-200'}"
    on:click={() => (mobileTab = 'order')}
  >
    {$t('dex.placeOrder')}
  </button>
</div>

<style>
  .mobile-tabs {
    position: sticky;
    top: 0;
    z-index: 10;
  }
</style>
