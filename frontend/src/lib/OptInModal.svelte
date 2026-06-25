<script>
  import { createEventDispatcher, tick } from 'svelte';
  import { api } from '../api.js';
  import { formatRawAmount } from '../amount.js';
  import { t } from '../i18n/index.js';
  import AssetSearch from './AssetSearch.svelte';

  const dispatch = createEventDispatcher();

  export let wallet;

  let step = 'form';
  let assetId = '';
  let selectedAsset = null;
  let pin = '';
  let preview = null;
  let result = null;
  let error = '';
  let assetInput;
  let pinInput;

  tick().then(() => assetInput?.focus());

  function close() {
    if (step !== 'submitting') dispatch('close');
  }

  function onAssetSelect(asset) {
    selectedAsset = asset;
    if (asset) {
      assetId = String(asset.id);
    } else {
      assetId = '';
    }
  }

  async function prepare() {
    error = '';
    const parsedAssetId = Number(assetId);
    if (!Number.isSafeInteger(parsedAssetId) || parsedAssetId <= 0) {
      error = $t('optIn.invalidAssetId');
      return;
    }
    step = 'preparing';
    try {
      preview = await api.prepareOptIn({
        address: wallet.first_address,
        assetId: parsedAssetId,
      });
      step = 'review';
      await tick();
      pinInput?.focus();
    } catch (e) {
      error = e.message;
      step = 'form';
    }
  }

  function back() {
    step = 'form';
    preview = null;
    pin = '';
    error = '';
  }

  async function submit() {
    if (!pin) {
      error = $t('transfer.pinRequired');
      return;
    }
    error = '';
    step = 'submitting';
    try {
      result = await api.optInAsset({
        walletId: wallet.id,
        pin,
        address: wallet.first_address,
        assetId: Number(assetId),
      });
      pin = '';
      step = 'success';
      dispatch('completed', result);
    } catch (e) {
      error = e.message;
      step = 'review';
    }
  }
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
  on:click|self={close}
  on:keydown={(event) => event.key === 'Escape' && close()}
  role="presentation"
>
  <div
    class="card max-h-[90vh] w-full max-w-lg overflow-y-auto"
    role="dialog"
    aria-modal="true"
    aria-labelledby="opt-in-title"
  >
    <div class="mb-5 flex items-center justify-between">
      <div>
        <h3 id="opt-in-title" class="text-lg font-semibold text-gray-100">{$t('optIn.title')}</h3>
        <p class="mt-1 text-xs text-gray-500">{$t('optIn.subtitle')}</p>
      </div>
      <button class="rounded p-1 text-gray-500 hover:text-gray-300" on:click={close} aria-label={$t('common.cancel')}>
        <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
          <path stroke-linecap="round" d="M6 18 18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    {#if step === 'form' || step === 'preparing'}
      <div class="space-y-4">
        <div>
          <label class="label" for="opt-in-asset">{$t('optIn.assetId')}</label>
          <div bind:this={assetInput}>
            <AssetSearch
              placeholder={$t('optIn.assetIdPlaceholder')}
              allowManualEntry={true}
              on:select={onAssetSelect}
            />
          </div>
        </div>
        <div class="rounded-lg border border-yellow-500/20 bg-yellow-500/5 px-4 py-3 text-xs text-yellow-200">
          {$t('optIn.balanceNotice')}
        </div>
        {#if error}
          <div class="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-400">{error}</div>
        {/if}
        <button class="btn-primary w-full" on:click={prepare} disabled={step === 'preparing'}>
          {step === 'preparing' ? $t('transfer.preparing') : $t('optIn.review')}
        </button>
      </div>
    {:else if step === 'review' || step === 'submitting'}
      <div class="space-y-4">
        <div class="rounded-xl border border-gray-700 bg-gray-900/50 p-4">
          <dl class="space-y-2 text-sm">
            <div class="flex justify-between gap-4">
              <dt class="text-gray-500">{$t('transfer.asset')}</dt>
              <dd class="text-gray-200">{preview.preview.asset_name}</dd>
            </div>
            <div class="flex justify-between gap-4">
              <dt class="text-gray-500">{$t('optIn.assetId')}</dt>
              <dd class="font-mono text-gray-200">{preview.preview.asset_id}</dd>
            </div>
            <div class="flex justify-between gap-4">
              <dt class="text-gray-500">{$t('transfer.fee')}</dt>
              <dd class="font-mono text-gray-200">{formatRawAmount(BigInt(preview.preview.fee), 6)} ALGO</dd>
            </div>
          </dl>
        </div>
        <div>
          <label class="label" for="opt-in-pin">{$t('transfer.pin')}</label>
          <input
            bind:this={pinInput}
            id="opt-in-pin"
            type="password"
            class="input"
            bind:value={pin}
            placeholder={$t('transfer.pinPlaceholder')}
            autocomplete="current-password"
            disabled={step === 'submitting'}
          />
        </div>
        {#if error}
          <div class="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-400">{error}</div>
        {/if}
        <div class="flex gap-3">
          <button class="btn-secondary flex-1" on:click={back} disabled={step === 'submitting'}>{$t('common.back')}</button>
          <button class="btn-primary flex-1" on:click={submit} disabled={!pin || step === 'submitting'}>
            {step === 'submitting' ? $t('optIn.submitting') : $t('optIn.confirm')}
          </button>
        </div>
      </div>
    {:else}
      <div class="space-y-5 text-center">
        <div class="mx-auto flex h-14 w-14 items-center justify-center rounded-full bg-green-500/10 text-green-400">
          <svg class="h-8 w-8" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="m4.5 12.75 6 6 9-13.5" />
          </svg>
        </div>
        <div>
          <h4 class="text-lg font-semibold text-gray-100">{$t('optIn.success')}</h4>
          <p class="mt-1 text-sm text-gray-500">{$t('transfer.confirmedRound', { round: result.confirmed_round })}</p>
        </div>
        <p class="break-all rounded-lg border border-gray-700 bg-gray-900/50 p-3 font-mono text-xs text-gray-400">{result.txid}</p>
        <button class="btn-primary w-full" on:click={close}>{$t('transfer.done')}</button>
      </div>
    {/if}
  </div>
</div>
