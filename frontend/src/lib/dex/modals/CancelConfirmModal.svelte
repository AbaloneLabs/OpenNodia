<script>
  import { formatRawAmount } from '../../../amount.js';
  import { t } from '../../../i18n/index.js';

  export let cancelTarget = null;
  export let cancelPrepare = null;
  export let cancelBatch = [];
  export let cancelPin = '';
  export let submitting = false;
  export let focusTrap = () => {};
  export let closeModals = () => {};
  export let submitCancel = () => {};
  export let cancelBatchRecoverableAlgo = () => 0;
  export let cancelBatchRecoverableAssets = () => [];
  export let fmtAssetAmount = (raw) => String(raw ?? '—');
</script>

{#if cancelTarget && cancelPrepare}
  <div class="modal-overlay fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
    <div
      class="modal-content w-full max-w-lg rounded-lg border border-gray-700 bg-gray-900 p-6"
      use:focusTrap
      role="dialog"
      aria-modal="true"
      aria-labelledby="dex-cancel-dialog-title"
      tabindex="-1"
    >
      <h3 id="dex-cancel-dialog-title" class="mb-4 text-base font-semibold text-gray-200">
        {cancelBatch.length > 1 ? $t('dex.reviewCancelBatch') : $t('dex.reviewCancel')}
      </h3>
      <div class="mb-4 space-y-2 text-sm text-gray-400">
        {#if cancelBatch.length > 1}
          <div>{$t('dex.cancelBatchCount', { count: cancelBatch.length })}</div>
          <div>
            {$t('dex.recoverableAlgo')}: <span class="text-gray-200">{formatRawAmount(cancelBatchRecoverableAlgo(), 6)}</span>
          </div>
          {#each cancelBatchRecoverableAssets() as [assetId, amount]}
            <div>
              {$t('dex.recoverableAsset')}: <span class="text-gray-200">{fmtAssetAmount(amount, assetId)} (#{assetId})</span>
            </div>
          {/each}
          <div class="max-h-28 space-y-1 overflow-y-auto rounded border border-gray-800 bg-gray-950/40 p-2">
            {#each cancelBatch as order}
              <div class="break-all font-mono text-[10px] text-gray-500">
                #{Number(order.split_index || 0) + 1} {order.escrow_addr}
              </div>
            {/each}
          </div>
        {:else}
          <div>{$t('dex.escrowAddress')}: <span class="break-all text-xs text-gray-300">{cancelTarget.escrow_addr}</span></div>
          <div>
            {$t('dex.recoverableAlgo')}: <span class="text-gray-200">{formatRawAmount(cancelPrepare.recoverable_algo, 6)}</span>
          </div>
          {#if cancelPrepare.recoverable_asset}
            <div>
              {$t('dex.recoverableAsset')}: <span class="text-gray-200">
                {fmtAssetAmount(cancelPrepare.recoverable_asset[1], cancelPrepare.recoverable_asset[0])} (#{cancelPrepare.recoverable_asset[0]})
              </span>
            </div>
          {/if}
        {/if}
        <label class="mt-3 block">
          <span class="mb-1 block text-xs text-gray-400">{$t('transfer.pin')}</span>
          <input
            class="input-field"
            type="password"
            autocomplete="off"
            bind:value={cancelPin}
            placeholder={$t('transfer.pinPlaceholder')}
          />
        </label>
      </div>
      <div class="flex gap-3">
        <button class="btn-secondary flex-1" on:click={closeModals} disabled={submitting}>
          {$t('common.cancel')}
        </button>
        <button class="btn-primary flex-1" on:click={submitCancel} disabled={submitting || !cancelPin}>
          {submitting
            ? $t('dex.submitting') + '...'
            : cancelBatch.length > 1
              ? $t('dex.confirmCancelBatch')
              : $t('dex.confirmCancel')}
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
