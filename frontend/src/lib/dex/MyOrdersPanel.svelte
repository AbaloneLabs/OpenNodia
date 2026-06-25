<script>
  import { t } from '../../i18n/index.js';

  export let ordersTab = 'active';
  export let myOrdersLoading = false;
  export let activeOrders = [];
  export let historyOrders = [];
  export let loading = false;
  export let submitting = false;
  export let fmtAssetLabel = (value) => String(value ?? '');
  export let fmtPrice = (value) => String(value ?? '—');
  export let fmtAssetAmount = (value) => String(value ?? '—');
  export let statusColor = () => 'text-gray-400 bg-gray-500/10';
  export let statusLabel = (status) => status || '—';
  export let splitChildCount = () => 0;
  export let splitProgress = () => ({ pct: 0 });
  export let splitActiveOrders = () => [];
  export let isFirstActiveSplitChild = () => false;
  export let prepareFill = () => {};
  export let prepareCancel = () => {};
  export let prepareCancelMany = () => {};
  export let copyOrderLink = () => {};
</script>

<div class="card p-4">
  <div class="mb-3 flex items-center justify-between">
    <h3 class="text-sm font-semibold text-gray-200">{$t('dex.myOrders')}</h3>
    <div class="flex gap-1 rounded-md bg-gray-800/50 p-0.5">
      <button
        class="rounded px-2 py-1 text-xs font-medium transition {ordersTab === 'active'
          ? 'bg-gray-700 text-gray-200'
          : 'text-gray-500 hover:text-gray-300'}"
        on:click={() => (ordersTab = 'active')}
      >
        {$t('dex.active')}
      </button>
      <button
        class="rounded px-2 py-1 text-xs font-medium transition {ordersTab === 'history'
          ? 'bg-gray-700 text-gray-200'
          : 'text-gray-500 hover:text-gray-300'}"
        on:click={() => (ordersTab = 'history')}
      >
        {$t('dex.history')}
      </button>
    </div>
  </div>

  {#if myOrdersLoading}
    <div class="py-8 text-center text-xs text-gray-500">{$t('dex.loading')}</div>
  {:else if ordersTab === 'active' && activeOrders.length === 0}
    <div class="py-8 text-center text-xs text-gray-600">{$t('dex.noOrders')}</div>
  {:else if ordersTab === 'history' && historyOrders.length === 0}
    <div class="py-8 text-center text-xs text-gray-600">{$t('dex.noOrders')}</div>
  {:else}
    <div class="space-y-2">
      {#each (ordersTab === 'active' ? activeOrders : historyOrders) as order}
        <div class="rounded-md border border-gray-700/50 bg-gray-800/30 p-2.5">
          <div class="mb-1.5 flex items-center justify-between">
            <div class="flex items-center gap-2">
              <span class="text-xs font-medium {order.side === 'sell' ? 'text-red-300' : 'text-green-300'}">
                {order.side === 'sell' ? $t('dex.sell') : $t('dex.buy')}
              </span>
              <span class="font-mono text-[10px] text-gray-500">
                {fmtAssetLabel(order.sell_asset)}/{fmtAssetLabel(order.buy_asset)}
              </span>
            </div>
            <span class="rounded px-1.5 py-0.5 text-[10px] {statusColor(order.status)}">
              {statusLabel(order.status)}
            </span>
          </div>
          {#if order.parent_id}
            <div class="mb-1.5 rounded border border-blue-500/20 bg-blue-500/5 px-2 py-1">
              <div class="flex items-center justify-between gap-2 text-[10px] text-blue-200">
                <span>
                  {$t('dex.splitChild', {
                    index: Number(order.split_index || 0) + 1,
                    count: splitChildCount(order),
                  })}
                </span>
                <span>{$t('dex.splitGroupProgress', { pct: splitProgress(order).pct })}</span>
              </div>
              <div class="mt-1 h-1 w-full overflow-hidden rounded-full bg-blue-950/70">
                <div class="h-full rounded-full bg-blue-400" style="width: {splitProgress(order).pct}%"></div>
              </div>
            </div>
          {/if}
          <div class="grid grid-cols-3 gap-1 font-mono text-[11px] text-gray-400">
            <div>
              <div class="text-[9px] uppercase text-gray-600">{$t('dex.price')}</div>
              <div class="text-gray-300">{fmtPrice(order.price)}</div>
            </div>
            <div>
              <div class="text-[9px] uppercase text-gray-600">{$t('dex.amount')}</div>
              <div class="text-gray-300">{fmtAssetAmount(order.sell_amount, order.sell_asset)}</div>
            </div>
            <div>
              <div class="text-[9px] uppercase text-gray-600">{$t('dex.fillPct')}</div>
              <div class="text-gray-300">
                {order.sell_amount > 0 ? Math.round((order.filled_amount / order.sell_amount) * 100) : 0}%
              </div>
            </div>
          </div>
          {#if order.sell_amount > 0}
            <div class="mt-1.5 h-1 w-full overflow-hidden rounded-full bg-gray-700">
              <div
                class="h-full rounded-full {order.side === 'sell' ? 'bg-red-500' : 'bg-green-500'}"
                style="width: {Math.round((order.filled_amount / order.sell_amount) * 100)}%"
              ></div>
            </div>
          {/if}
          {#if order.status === 'active'}
            <div class="mt-2 flex flex-wrap gap-2">
              <button
                class="min-w-[7rem] flex-1 rounded bg-green-500/20 px-2 py-1 text-xs text-green-300 hover:bg-green-500/30"
                on:click={() => prepareFill(order)}
                disabled={loading || submitting}
              >
                {$t('dex.fill')}
              </button>
              <button
                class="min-w-[7rem] flex-1 rounded bg-gray-600/20 px-2 py-1 text-xs text-gray-300 hover:bg-gray-600/30"
                on:click={() => prepareCancel(order)}
                disabled={loading || submitting}
              >
                {$t('dex.cancel')}
              </button>
              {#if isFirstActiveSplitChild(order) && splitActiveOrders(order).length > 1}
                <button
                  class="min-w-[7rem] flex-1 rounded bg-blue-500/20 px-2 py-1 text-xs text-blue-200 hover:bg-blue-500/30"
                  on:click={() => prepareCancelMany(splitActiveOrders(order))}
                  disabled={loading || submitting}
                >
                  {$t('dex.cancelSplitGroup', { count: splitActiveOrders(order).length })}
                </button>
              {/if}
            </div>
          {/if}
          <div class="mt-2">
            <button
              type="button"
              class="w-full rounded bg-gray-700/60 px-2 py-1 text-xs text-gray-300 hover:bg-gray-700"
              on:click={() => copyOrderLink(order)}
              disabled={loading || submitting}
            >
              {$t('dex.copyOrderLink')}
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
