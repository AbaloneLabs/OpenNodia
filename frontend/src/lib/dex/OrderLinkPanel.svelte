<script>
  import { t } from '../../i18n/index.js';

  export let orderLinkLoading = false;
  export let orderLinkDetail = null;
  export let orderLinkError = '';
  export let orderLinkUi = { canAct: false, labelKey: '' };
  export let loading = false;
  export let submitting = false;
  export let signer = '';
  export let copyOpenedOrderLink = () => {};
  export let prepareFill = () => {};
  export let prepareCancel = () => {};
  export let statusLabel = (status) => status || '—';
  export let fmtAssetLabel = (value) => String(value ?? '');
  export let fmtAssetAmount = (value) => String(value ?? '—');
</script>

{#if orderLinkLoading || orderLinkDetail || orderLinkError}
  <div class="mb-4 rounded-md border border-gray-700 bg-gray-900/80 p-4">
    <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
      <div>
        <h3 class="text-sm font-semibold text-gray-200">{$t('dex.orderLink')}</h3>
      </div>
      {#if orderLinkDetail}
        <button
          type="button"
          class="rounded bg-gray-700 px-2 py-1 text-xs text-gray-200 hover:bg-gray-600"
          on:click={copyOpenedOrderLink}
        >
          {$t('dex.copyOrderLink')}
        </button>
      {/if}
    </div>
    {#if orderLinkLoading}
      <div class="py-3 text-xs text-gray-500">{$t('dex.loading')}</div>
    {:else if orderLinkError}
      <div class="rounded border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-300">
        {orderLinkError}
      </div>
    {:else if orderLinkDetail}
      <div class="grid gap-3 text-xs text-gray-400 sm:grid-cols-2">
        <div>
          <div class="text-[10px] uppercase text-gray-600">{$t('dex.status')}</div>
          <div class="mt-0.5 font-mono text-gray-200">{statusLabel(orderLinkDetail.status)}</div>
        </div>
        <div>
          <div class="text-[10px] uppercase text-gray-600">{$t('dex.verified')}</div>
          <div class="mt-0.5 {orderLinkUi.canAct ? 'text-green-400' : 'text-yellow-400'}">
            {$t(orderLinkUi.labelKey)}
          </div>
        </div>
        <div>
          <div class="text-[10px] uppercase text-gray-600">{$t('dex.side')}</div>
          <div class="mt-0.5 text-gray-200">
            {orderLinkDetail.decoded.side === 'sell' ? $t('dex.sell') : $t('dex.buy')}
            {fmtAssetLabel(orderLinkDetail.decoded.sell_asset)} → {fmtAssetLabel(orderLinkDetail.decoded.buy_asset)}
          </div>
        </div>
        <div>
          <div class="text-[10px] uppercase text-gray-600">{$t('dex.amount')}</div>
          <div class="mt-0.5 text-gray-200">
            {fmtAssetAmount(orderLinkDetail.decoded.sell_amount, orderLinkDetail.decoded.sell_asset)}
            →
            {fmtAssetAmount(orderLinkDetail.decoded.buy_amount, orderLinkDetail.decoded.buy_asset)}
          </div>
        </div>
        <div class="sm:col-span-2">
          <div class="text-[10px] uppercase text-gray-600">{$t('dex.escrowAddress')}</div>
          <div class="mt-0.5 break-all font-mono text-gray-300">{orderLinkDetail.decoded.escrow}</div>
        </div>
        {#if orderLinkDetail.verification}
          <div class="sm:col-span-2 rounded bg-gray-800/60 px-3 py-2">
            <span class={orderLinkDetail.verification.valid ? 'text-green-400' : 'text-yellow-400'}>
              {orderLinkDetail.verification.valid
                ? $t('dex.orderLinkLedgerActive')
                : orderLinkDetail.verification.mismatch_reason}
            </span>
          </div>
        {/if}
        {#if orderLinkDetail.resolution}
          <div class="sm:col-span-2 rounded bg-gray-800/60 px-3 py-2">
            <div class="text-gray-300">{statusLabel(orderLinkDetail.resolution.status)}</div>
            {#if orderLinkDetail.resolution.tx_id}
              <div class="mt-1 break-all font-mono text-[11px] text-gray-500">
                {orderLinkDetail.resolution.tx_id}
              </div>
            {/if}
            {#if orderLinkDetail.resolution.round}
              <div class="mt-1 font-mono text-[11px] text-gray-500">
                {$t('dashboard.lastRound')}: {orderLinkDetail.resolution.round}
              </div>
            {/if}
          </div>
        {/if}
        {#if orderLinkDetail.error}
          <div class="sm:col-span-2 rounded bg-red-500/10 px-3 py-2 text-red-300">
            {orderLinkDetail.error}
          </div>
        {/if}
      </div>
      {#if orderLinkUi.canAct && orderLinkDetail.order?.status === 'active'}
        <div class="mt-3 flex gap-2">
          <button
            type="button"
            class="rounded bg-green-500/20 px-3 py-1.5 text-xs text-green-300 hover:bg-green-500/30"
            on:click={() => prepareFill(orderLinkDetail.order)}
            disabled={loading || submitting || !signer}
          >
            {$t('dex.fill')}
          </button>
          <button
            type="button"
            class="rounded bg-gray-600/20 px-3 py-1.5 text-xs text-gray-300 hover:bg-gray-600/30"
            on:click={() => prepareCancel(orderLinkDetail.order)}
            disabled={loading || submitting || !signer}
          >
            {$t('dex.cancel')}
          </button>
        </div>
      {/if}
    {/if}
  </div>
{/if}
