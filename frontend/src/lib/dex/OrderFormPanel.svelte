<script>
  import { t } from '../../i18n/index.js';
  import RouteCandidatesPanel from './RouteCandidatesPanel.svelte';

  export let side = 'buy';
  export let quoteAssetId = '';
  export let quoteLabel = '';
  export let baseLabel = '';
  export let orderPrice = '';
  export let orderAmount = '';
  export let orderTotal = '';
  export let expiryPresets = [];
  export let expiryPreset = '1d';
  export let expireRounds = 10000;
  export let estimatedExpiry = '';
  export let splitCount = 1;
  export let iocMode = false;
  export let fillThenPlace = false;
  export let effectiveSplitCount = 1;
  export let perOrderAmount = '0';
  export let estimatedEscrowCost = '0';
  export let routeCandidates = null;
  export let routeLoading = false;
  export let routeError = '';
  export let commonQuoteStatus = { state: 'idle', stale: false, source: '', message: '' };
  export let loading = false;
  export let switchSide = () => {};
  export let setMaxAmount = () => {};
  export let selectExpiryPreset = () => {};
  export let onExpireRoundsInput = () => {};
  export let refreshRouteCandidates = () => {};
  export let prepareCreate = () => {};
  export let fmtAssetAmount = (raw) => String(raw ?? '—');
</script>

<div class="card p-4">
  <div class="mb-4 grid grid-cols-2 gap-1 rounded-lg bg-gray-800/50 p-1">
    <button
      class="rounded-md py-2 text-sm font-semibold transition {side === 'buy'
        ? 'bg-green-500/20 text-green-300'
        : 'text-gray-400 hover:text-gray-200'}"
      on:click={() => switchSide('buy')}
    >
      {$t('dex.buy')}
    </button>
    <button
      class="rounded-md py-2 text-sm font-semibold transition {side === 'sell'
        ? 'bg-red-500/20 text-red-300'
        : 'text-gray-400 hover:text-gray-200'}"
      on:click={() => switchSide('sell')}
    >
      {$t('dex.sell')}
    </button>
  </div>

  {#if !quoteAssetId}
    <div class="py-8 text-center text-xs text-gray-500">{$t('dex.selectPair')}</div>
  {:else}
    <label class="mb-3 block">
      <span class="mb-1 block text-xs text-gray-400">{$t('dex.price')} ({quoteLabel})</span>
      <input
        class="input-field font-mono"
        type="number"
        min="0"
        step="0.000001"
        placeholder="0.000000"
        bind:value={orderPrice}
      />
    </label>

    <label class="mb-3 block">
      <div class="mb-1 flex items-center justify-between">
        <span class="text-xs text-gray-400">{$t('dex.amount')} ({baseLabel})</span>
        <button class="text-[10px] font-medium text-algo-400 hover:text-algo-300" on:click={setMaxAmount}>
          {$t('dex.max')}
        </button>
      </div>
      <input
        class="input-field font-mono"
        type="number"
        min="0"
        step="0.000001"
        placeholder="0.000000"
        bind:value={orderAmount}
      />
    </label>

    <div class="mb-4 flex items-center justify-between rounded-md bg-gray-800/50 px-3 py-2">
      <span class="text-xs text-gray-400">{$t('dex.total')}</span>
      <span class="font-mono text-sm text-gray-200">
        {orderTotal || '0.000000'} {quoteLabel}
      </span>
    </div>

    <div class="mb-4">
      <span class="mb-1.5 block text-xs text-gray-400">{$t('dex.expiryDuration')}</span>
      <div class="flex flex-wrap gap-1.5">
        {#each expiryPresets as preset}
          <button
            type="button"
            class="rounded-md px-2.5 py-1 text-xs font-medium transition {expiryPreset === preset.id
              ? 'bg-indigo-600 text-white'
              : 'bg-gray-800 text-gray-300 hover:bg-gray-700'}"
            on:click={() => selectExpiryPreset(preset.id)}
          >
            {$t(preset.labelKey)}
          </button>
        {/each}
      </div>
      <p class="mt-1.5 text-[11px] leading-snug text-gray-500">
        {$t('dex.approxRounds', { count: Number(expireRounds || 0).toLocaleString() })}
        <br />{$t('dex.approxExpiry')}
      </p>
      <div class="mt-3 grid grid-cols-2 gap-3">
        <div>
          <label class="block">
            <span class="mb-1 block text-xs text-gray-400">{$t('dex.expireRounds')}</span>
            <input
              class="input-field font-mono"
              type="number"
              min="3"
              max="1000000"
              value={expireRounds}
              on:input={onExpireRoundsInput}
            />
          </label>
          <p class="mt-1 text-[11px] text-gray-500">{estimatedExpiry}</p>
        </div>
        <div>
          <label class="block">
            <span class="mb-1 block text-xs text-gray-400">{$t('dex.splitCount')}</span>
            <input
              class="input-field font-mono disabled:cursor-not-allowed disabled:opacity-50"
              type="number"
              min="1"
              max="20"
              bind:value={splitCount}
              disabled={iocMode}
            />
          </label>
          {#if iocMode}
            <p class="mt-1 text-[11px] text-gray-500">
              {$t('dex.iocNoStandingSplitHint')}
            </p>
          {:else}
            <p class="mt-1 text-[11px] leading-snug text-gray-500">
              {$t('dex.splitsPreview', {
                count: effectiveSplitCount,
                amount: perOrderAmount,
                unit: baseLabel,
                cost: estimatedEscrowCost,
              })}
            </p>
          {/if}
        </div>
      </div>
    </div>

    <label class="mb-4 flex cursor-pointer items-center gap-2 text-xs text-gray-400">
      <input type="checkbox" class="h-3.5 w-3.5 rounded" bind:checked={iocMode} />
      <span>{$t('dex.iocToggle')}</span>
    </label>
    <label class="mb-4 flex cursor-pointer items-center gap-2 text-xs text-gray-400 {iocMode ? 'opacity-50' : ''}">
      <input
        type="checkbox"
        class="h-3.5 w-3.5 rounded"
        bind:checked={fillThenPlace}
        disabled={iocMode}
      />
      <span>{$t('dex.fillThenPlaceToggle')}</span>
    </label>

    <RouteCandidatesPanel
      {routeCandidates}
      {routeLoading}
      {routeError}
      {orderAmount}
      {orderPrice}
      {commonQuoteStatus}
      {refreshRouteCandidates}
      {fmtAssetAmount}
    />

    <button
      class="w-full rounded-lg py-2.5 font-semibold text-white transition disabled:opacity-50 {side === 'buy'
        ? 'bg-green-600 hover:bg-green-700'
        : 'bg-red-600 hover:bg-red-700'}"
      on:click={prepareCreate}
      disabled={loading}
    >
      {loading
        ? $t('common.confirm') + '...'
        : side === 'buy'
          ? $t('dex.buyBtn', { unit: quoteLabel })
          : $t('dex.sellBtn', { unit: quoteLabel })}
    </button>
  {/if}
</div>

<style>
  .input-field {
    @apply w-full rounded-md border border-gray-700 bg-gray-800 px-3 py-2 text-sm text-gray-200;
  }

  .input-field:focus {
    @apply border-algo-500 outline-none;
  }
</style>
