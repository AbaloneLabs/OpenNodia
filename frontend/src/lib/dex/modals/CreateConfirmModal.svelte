<script>
  import { t } from '../../../i18n/index.js';

  export let pendingPrepare = null;
  export let createPin = '';
  export let submitting = false;
  export let focusTrap = () => {};
  export let closeModals = () => {};
  export let submitCreate = () => {};
  export let fmtAssetAmount = (raw) => String(raw ?? '—');
</script>

{#if pendingPrepare}
  <div class="modal-overlay fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
    <div
      class="modal-content w-full max-w-lg rounded-lg border border-gray-700 bg-gray-900 p-6"
      use:focusTrap
      role="dialog"
      aria-modal="true"
      aria-labelledby="dex-create-dialog-title"
      tabindex="-1"
    >
      <h3 id="dex-create-dialog-title" class="mb-4 text-base font-semibold text-gray-200">
        {pendingPrepare.routed
          ? pendingPrepare.ioc
            ? $t('dex.reviewRoute')
            : $t('dex.reviewFillThenPlace')
          : $t('dex.reviewCreate')}
      </h3>
      <div class="mb-4 space-y-2 text-sm text-gray-400">
        {#if pendingPrepare.routed}
          {#if pendingPrepare.router}
            <div>{$t('dex.routeDecision')}: <span class="text-gray-200">{pendingPrepare.source_type}</span></div>
            <div>{$t('dex.totalReceived')}: <span class="text-gray-200">{pendingPrepare.selected.amount_out}</span></div>
            <div>{$t('dex.totalCost')}: <span class="text-gray-200">{pendingPrepare.selected.amount_in}</span></div>
            <div>{$t('dex.routeCandidateMin')}: <span class="text-gray-200">{pendingPrepare.selected.minimum_out}</span></div>
            {#if pendingPrepare.selected.split_legs?.length}
              <div class="rounded bg-gray-800/50 px-3 py-2 text-xs">
                <div class="mb-1 text-gray-300">{$t('dex.routeAtomicSplit')}</div>
                <div class="space-y-1">
                  {#each pendingPrepare.selected.split_legs as leg}
                    <div class="flex justify-between gap-2">
                      <span class="min-w-0 truncate">{leg.source_label} #{leg.app_id}</span>
                      <span class="shrink-0 font-mono text-gray-300">
                        {fmtAssetAmount(leg.amount_in, leg.asset_in)} → {fmtAssetAmount(leg.minimum_out, leg.asset_out)}
                      </span>
                    </div>
                  {/each}
                </div>
              </div>
            {/if}
            {#each (pendingPrepare.txs || []) as tx, i}
              <div class="rounded bg-gray-800/50 px-3 py-2 text-xs">
                <span class="text-gray-500">#{i + 1} {tx.ty}</span>: {tx.summary}
              </div>
            {/each}
          {:else}
            <div>{$t('dex.routeDecision')}: <span class="text-gray-200">{pendingPrepare.decision}</span></div>
            <div>{$t('dex.totalReceived')}: <span class="text-gray-200">{pendingPrepare.total_received}</span></div>
            <div>{$t('dex.totalCost')}: <span class="text-gray-200">{pendingPrepare.total_cost}</span></div>
          {/if}
          {#if pendingPrepare.ioc && pendingPrepare.remaining > 0}
            <div class="text-yellow-400">{$t('dex.routeRemaining', { amount: pendingPrepare.remaining })}</div>
          {/if}
          {#if pendingPrepare.placeRemaining}
            <div class="text-blue-300">
              {$t('dex.fillThenPlaceCreates', { count: pendingPrepare.new_orders_needed || 0 })}
            </div>
          {:else}
            <div class="mt-3 text-xs text-gray-500">{$t('dex.iocDiscarded')}</div>
          {/if}
          {#if pendingPrepare.fills?.length > 1}
            <div class="mt-2 text-xs text-yellow-400">{$t('dex.iocSequentialWarning')}</div>
          {/if}
        {:else}
          <div>{$t('dex.kind')}: <span class="text-gray-200">{pendingPrepare.kind}</span></div>
          <div>
            {$t('dex.escrowAddress')}: <span class="break-all text-xs text-gray-300">{pendingPrepare.escrow_address}</span>
          </div>
          <div class="mt-3 text-xs text-gray-500">
            {$t('dex.txsToSign', { count: pendingPrepare.owner_txs.length })}
          </div>
          {#each pendingPrepare.owner_txs as tx, i}
            <div class="rounded bg-gray-800/50 px-3 py-2 text-xs">
              <span class="text-gray-500">#{i} {tx.ty}</span>: {tx.summary}
            </div>
          {/each}
        {/if}
        <label class="mt-3 block">
          <span class="mb-1 block text-xs text-gray-400">{$t('transfer.pin')}</span>
          <input
            class="input-field"
            type="password"
            autocomplete="off"
            bind:value={createPin}
            placeholder={$t('transfer.pinPlaceholder')}
          />
        </label>
      </div>
      <div class="flex gap-3">
        <button class="btn-secondary flex-1" on:click={closeModals} disabled={submitting}>
          {$t('common.cancel')}
        </button>
        <button class="btn-primary flex-1" on:click={submitCreate} disabled={submitting || !createPin}>
          {submitting ? $t('dex.submitting') + '...' : $t('dex.confirmCreate')}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .input-field {
    @apply w-full rounded-md border border-gray-700 bg-gray-800 px-3 py-2 text-sm text-gray-200;
  }

  .input-field:focus {
    @apply border-algo-500 outline-none;
  }

  .modal-overlay {
    animation: fadeIn 0.15s ease-out;
  }

  .modal-content {
    animation: slideUp 0.2s ease-out;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes slideUp {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
