<script>
  import { t } from '../../../i18n/index.js';

  export let fillTarget = null;
  export let fillPrepare = null;
  export let fillPin = '';
  export let submitting = false;
  export let focusTrap = () => {};
  export let closeModals = () => {};
  export let submitFill = () => {};
</script>

{#if fillTarget && fillPrepare}
  <div class="modal-overlay fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
    <div
      class="modal-content w-full max-w-lg rounded-lg border border-gray-700 bg-gray-900 p-6"
      use:focusTrap
      role="dialog"
      aria-modal="true"
      aria-labelledby="dex-fill-dialog-title"
      tabindex="-1"
    >
      <h3 id="dex-fill-dialog-title" class="mb-4 text-base font-semibold text-gray-200">{$t('dex.reviewFill')}</h3>
      <div class="mb-4 space-y-2 text-sm text-gray-400">
        <div>{$t('dex.escrowAddress')}: <span class="break-all text-xs text-gray-300">{fillTarget.escrow_addr}</span></div>
        <div>{$t('dex.fillerTx')}: <span class="text-xs text-gray-300">{fillPrepare.filler_tx.summary}</span></div>
        {#if fillPrepare.verification}
          <div class="mt-2 text-xs">
            {$t('dex.verified')}:
            <span class={fillPrepare.verification.valid ? 'text-green-400' : 'text-red-400'}>
              {fillPrepare.verification.valid ? $t('common.confirm') : fillPrepare.verification.mismatch_reason}
            </span>
          </div>
        {/if}
        <label class="mt-3 block">
          <span class="mb-1 block text-xs text-gray-400">{$t('transfer.pin')}</span>
          <input
            class="input-field"
            type="password"
            autocomplete="off"
            bind:value={fillPin}
            placeholder={$t('transfer.pinPlaceholder')}
          />
        </label>
      </div>
      <div class="flex gap-3">
        <button class="btn-secondary flex-1" on:click={closeModals} disabled={submitting}>
          {$t('common.cancel')}
        </button>
        <button class="btn-primary flex-1" on:click={submitFill} disabled={submitting || !fillPin}>
          {submitting ? $t('dex.submitting') + '...' : $t('dex.confirmFill')}
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
