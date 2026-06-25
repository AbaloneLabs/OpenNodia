<script>
  import { t } from '../../i18n/index.js';

  export let mobileTab = 'order';
  export let trades = [];
  export let loadTrades = () => {};
  export let relTime = () => '—';
  export let fmtPrice = (value) => String(value ?? '—');
  export let fmtBaseAmount = (value) => String(value ?? '—');
</script>

<div class="card p-4 lg:block {mobileTab === 'orderbook' ? 'block' : 'hidden'}">
  <div class="mb-3 flex items-center justify-between">
    <h3 class="text-sm font-semibold text-gray-200">{$t('dex.recentTrades')}</h3>
    <button class="text-xs text-gray-500 underline hover:text-gray-300" on:click={loadTrades}>
      {$t('common.retry')}
    </button>
  </div>
  {#if trades.length > 0}
    <div class="grid grid-cols-4 px-2 py-1 text-[10px] font-medium uppercase tracking-wider text-gray-500">
      <span>{$t('dex.time')}</span>
      <span class="text-center">{$t('dex.side')}</span>
      <span class="text-right">{$t('dex.price')}</span>
      <span class="text-right">{$t('dex.amount')}</span>
    </div>
    <div class="space-y-0.5">
      {#each trades.slice(0, 12) as trade}
        <div class="grid grid-cols-4 px-2 py-0.5 font-mono text-xs">
          <span class="text-gray-500">{relTime(trade.timestamp)}</span>
          <span class="text-center">
            <span class={trade.side === 'buy' ? 'text-green-400' : 'text-red-400'}>
              {trade.side === 'buy' ? $t('dex.buy') : $t('dex.sell')}
            </span>
          </span>
          <span class="text-right text-gray-300">{fmtPrice(trade.price)}</span>
          <span class="text-right text-gray-400">{fmtBaseAmount(trade.amount)}</span>
        </div>
      {/each}
    </div>
  {:else}
    <div class="py-4 text-center text-xs text-gray-600">{$t('dex.noTrades')}</div>
  {/if}
</div>
