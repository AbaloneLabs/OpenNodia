<script>
  import { createEventDispatcher, tick } from 'svelte';
  import { api } from '../api.js';
  import {
    formatRawAmount,
    parseDecimalToRaw,
    rawToSafeNumber,
    utf8ByteLength,
  } from '../amount.js';
  import { t } from '../i18n/index.js';

  const dispatch = createEventDispatcher();

  export let wallet;
  export let asset;
  export let accountInfo;
  export let algoAsset;

  let step = 'form';
  let recipient = '';
  let amount = '';
  let note = '';
  let pin = '';
  let preview = null;
  let preparedRequest = null;
  let result = null;
  let error = '';
  let recipientInput;
  let pinInput;

  $: noteBytes = utf8ByteLength(note);
  $: decimals = asset?.decimals || 0;
  $: balanceLabel = asset ? formatRawAmount(BigInt(asset.amount || 0), decimals) : '0';
  $: spendableAlgo = Math.max(
    0,
    Number(accountInfo?.amount || algoAsset?.amount || 0) -
      Number(accountInfo?.min_balance || 0) -
      1000,
  );

  tick().then(() => recipientInput?.focus());

  function close() {
    if (step !== 'submitting') dispatch('close');
  }

  function setMax() {
    if (!asset) return;
    const raw = asset.id === 0 ? BigInt(spendableAlgo) : BigInt(asset.amount || 0);
    amount = formatRawAmount(raw, decimals);
  }

  function validateForm() {
    if (!recipient.trim()) throw new Error($t('transfer.recipientRequired'));
    if (noteBytes > 1024) throw new Error($t('transfer.noteTooLong'));

    const raw = parseDecimalToRaw(amount, decimals);
    if (raw > BigInt(asset.amount || 0)) {
      throw new Error($t('transfer.insufficientBalance'));
    }
    if (asset.id === 0 && raw > BigInt(spendableAlgo)) {
      throw new Error($t('transfer.insufficientSpendable'));
    }
    if (
      asset.id !== 0 &&
      Number(algoAsset?.amount || 0) - Number(accountInfo?.min_balance || 0) < 1000
    ) {
      throw new Error($t('transfer.insufficientFee'));
    }
    return rawToSafeNumber(raw);
  }

  async function prepare() {
    error = '';
    try {
      const rawAmount = validateForm();
      preparedRequest = {
        from: wallet.first_address,
        to: recipient.trim(),
        assetId: asset.id,
        amount: rawAmount,
        note: note.trim(),
      };
      step = 'preparing';
      preview = await api.prepareTransfer(preparedRequest);
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
    preparedRequest = null;
    pin = '';
    error = '';
  }

  async function submit() {
    if (!pin || !preparedRequest) {
      error = $t('transfer.pinRequired');
      return;
    }
    error = '';
    step = 'submitting';
    try {
      result = await api.sendTransfer({
        walletId: wallet.id,
        pin,
        ...preparedRequest,
      });
      pin = '';
      step = 'success';
      dispatch('completed', result);
    } catch (e) {
      error = e.message;
      step = 'review';
    }
  }

  async function copyTxId() {
    if (!result?.txid) return;
    await navigator.clipboard.writeText(result.txid);
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
    aria-labelledby="transfer-title"
  >
    <div class="mb-5 flex items-center justify-between">
      <div>
        <h3 id="transfer-title" class="text-lg font-semibold text-gray-100">
          {$t('transfer.title', { unit: asset.unit || asset.name })}
        </h3>
        <p class="mt-1 text-xs text-gray-500">
          {$t('transfer.balance')}: {balanceLabel} {asset.unit}
        </p>
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
          <label class="label" for="transfer-recipient">{$t('transfer.recipient')}</label>
          <input
            bind:this={recipientInput}
            id="transfer-recipient"
            class="input font-mono text-sm"
            bind:value={recipient}
            placeholder={$t('transfer.recipientPlaceholder')}
            spellcheck="false"
          />
        </div>

        <div>
          <div class="mb-1.5 flex items-center justify-between">
            <label class="label mb-0" for="transfer-amount">{$t('transfer.amount')}</label>
            <button class="text-xs text-algo-400 hover:text-algo-300" type="button" on:click={setMax}>
              {$t('transfer.max')}
            </button>
          </div>
          <div class="relative">
            <input
              id="transfer-amount"
              class="input pr-20"
              inputmode="decimal"
              bind:value={amount}
              placeholder="0"
            />
            <span class="absolute right-3 top-1/2 -translate-y-1/2 text-sm text-gray-500">{asset.unit}</span>
          </div>
          {#if asset.id === 0}
            <p class="mt-1 text-xs text-gray-600">
              {$t('transfer.spendable')}: {formatRawAmount(BigInt(spendableAlgo), 6)} ALGO
            </p>
          {/if}
        </div>

        <div>
          <div class="mb-1.5 flex items-center justify-between">
            <label class="label mb-0" for="transfer-note">{$t('transfer.note')}</label>
            <span class="text-xs {noteBytes > 1024 ? 'text-red-400' : 'text-gray-600'}">
              {noteBytes}/1024 bytes
            </span>
          </div>
          <textarea id="transfer-note" class="input" rows="3" bind:value={note} placeholder={$t('transfer.notePlaceholder')}></textarea>
        </div>

        {#if error}
          <div class="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-400">
            {error}
          </div>
        {/if}

        <button class="btn-primary w-full" on:click={prepare} disabled={step === 'preparing'}>
          {step === 'preparing' ? $t('transfer.preparing') : $t('transfer.review')}
        </button>
      </div>
    {:else if step === 'review' || step === 'submitting'}
      <div class="space-y-4">
        <div class="rounded-xl border border-gray-700 bg-gray-900/50 p-4">
          <h4 class="mb-3 text-sm font-medium text-gray-200">{$t('transfer.reviewTitle')}</h4>
          <dl class="space-y-2 text-sm">
            <div class="flex justify-between gap-4">
              <dt class="text-gray-500">{$t('transfer.asset')}</dt>
              <dd class="text-right text-gray-200">{preview.preview.asset_name} ({asset.unit})</dd>
            </div>
            <div class="flex justify-between gap-4">
              <dt class="text-gray-500">{$t('transfer.amount')}</dt>
              <dd class="font-mono text-gray-200">{formatRawAmount(BigInt(preview.preview.amount), decimals)} {asset.unit}</dd>
            </div>
            <div class="flex justify-between gap-4">
              <dt class="text-gray-500">{$t('transfer.fee')}</dt>
              <dd class="font-mono text-gray-200">{formatRawAmount(BigInt(preview.preview.fee), 6)} ALGO</dd>
            </div>
            <div>
              <dt class="mb-1 text-gray-500">{$t('transfer.recipient')}</dt>
              <dd class="break-all font-mono text-xs text-gray-300">{preview.preview.to}</dd>
            </div>
            {#if preview.preview.note}
              <div>
                <dt class="mb-1 text-gray-500">{$t('transfer.note')}</dt>
                <dd class="break-words text-gray-300">{preview.preview.note}</dd>
              </div>
            {/if}
          </dl>
        </div>

        <div>
          <label class="label" for="transfer-pin">{$t('transfer.pin')}</label>
          <input
            bind:this={pinInput}
            id="transfer-pin"
            type="password"
            class="input"
            bind:value={pin}
            placeholder={$t('transfer.pinPlaceholder')}
            autocomplete="current-password"
            disabled={step === 'submitting'}
          />
        </div>

        {#if error}
          <div class="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-400">
            {error}
          </div>
        {/if}

        <div class="flex gap-3">
          <button class="btn-secondary flex-1" on:click={back} disabled={step === 'submitting'}>
            {$t('common.back')}
          </button>
          <button class="btn-primary flex-1" on:click={submit} disabled={!pin || step === 'submitting'}>
            {step === 'submitting' ? $t('transfer.sending') : $t('transfer.confirmSend')}
          </button>
        </div>
      </div>
    {:else if step === 'success'}
      <div class="space-y-5 text-center">
        <div class="mx-auto flex h-14 w-14 items-center justify-center rounded-full bg-green-500/10 text-green-400">
          <svg class="h-8 w-8" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="m4.5 12.75 6 6 9-13.5" />
          </svg>
        </div>
        <div>
          <h4 class="text-lg font-semibold text-gray-100">{$t('transfer.success')}</h4>
          <p class="mt-1 text-sm text-gray-500">
            {$t('transfer.confirmedRound', { round: result.confirmed_round })}
          </p>
        </div>
        <button class="w-full break-all rounded-lg border border-gray-700 bg-gray-900/50 p-3 font-mono text-xs text-gray-400 hover:text-gray-200" on:click={copyTxId}>
          {result.txid}
        </button>
        <button class="btn-primary w-full" on:click={close}>{$t('transfer.done')}</button>
      </div>
    {/if}
  </div>
</div>
