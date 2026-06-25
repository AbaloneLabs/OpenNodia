<script>
  import { t } from '../../i18n/index.js';
  import QuoteStatus from '../QuoteStatus.svelte';

  export let pool = null;
  export let assetIn = '0';
  export let amountIn = '';
  export let slippageBps = 50;
  export let expireRounds = 1000;
  export let loadingQuote = false;
  export let loadingSwapPrepare = false;
  export let currentWallet = null;
  export let selectedPoolCanSwap = false;
  export let selectedPoolIsNative = false;
  export let quote = null;
  export let quoteIsStale = false;
  export let pairState = { quoteStatus: { state: 'idle', message: '', source: '' } };
  export let swapPreview = null;
  export let swapPin = '';
  export let loadingSwapSubmit = false;
  export let swapResult = null;
  export let assetLabel = (assetId) => `#${assetId}`;
  export let formatAsset = (raw) => String(raw ?? '0');
  export let fmt = (value) => String(value ?? '0');
  export let currentSwapAmountRawLabel = () => '—';
  export let loadQuote = () => {};
  export let prepareSwapExecution = () => {};
  export let submitSwapExecution = () => {};
</script>

<section class="card">
  <h3 class="text-lg font-semibold text-gray-100">{$t('lpTrade.swapQuote')}</h3>

  <div class="mt-4 grid gap-3 sm:grid-cols-4">
    <label class="block">
      <span class="label">{$t('lpTrade.assetIn')}</span>
      <select class="input mt-1" bind:value={assetIn}>
        <option value={String(pool.pool.asset_0)}>{assetLabel(pool.pool.asset_0)}</option>
        <option value={String(pool.pool.asset_1)}>{assetLabel(pool.pool.asset_1)}</option>
      </select>
    </label>
    <label class="block">
      <span class="label">{$t('lpTrade.rawAmountIn')}</span>
      <input class="input mt-1" bind:value={amountIn} inputmode="decimal" placeholder="1.0" />
      {#if amountIn}
        <span class="mt-1 block text-xs text-gray-500">
          {$t('lpTrade.rawAmountPreview', { amount: currentSwapAmountRawLabel() })}
        </span>
      {/if}
    </label>
    <label class="block">
      <span class="label">{$t('lpTrade.slippageBps')}</span>
      <input class="input mt-1" bind:value={slippageBps} inputmode="numeric" />
    </label>
    <label class="block">
      <span class="label">{$t('lpTrade.expireRounds')}</span>
      <input class="input mt-1" bind:value={expireRounds} inputmode="numeric" />
    </label>
  </div>

  <div class="mt-4 flex flex-wrap gap-3">
    <button class="btn-primary" type="button" on:click={loadQuote} disabled={loadingQuote || pool.pool.total_lp_supply === 0}>
      {loadingQuote ? $t('lpTrade.quoting') : $t('lpTrade.getQuote')}
    </button>
    <button class="btn-secondary" type="button" on:click={prepareSwapExecution} disabled={loadingSwapPrepare || !currentWallet || !selectedPoolCanSwap || pool.pool.total_lp_supply === 0 || !quote?.quote || quoteIsStale}>
      {loadingSwapPrepare ? $t('lpTrade.preparing') : $t('lpTrade.prepareSwap')}
    </button>
  </div>
  <p class="mt-3 text-xs text-gray-500">{$t('lpTrade.slippageDeadlineHint')}</p>
  {#if !selectedPoolIsNative && !selectedPoolCanSwap}
    <p class="mt-1 text-xs text-yellow-300">{$t('lpTrade.externalSwapReadOnly')}</p>
  {:else if !selectedPoolIsNative}
    <p class="mt-1 text-xs text-algo-300">{$t('lpTrade.externalSwapVerified')}</p>
  {/if}
  {#if quoteIsStale}
    <p class="mt-2 text-sm text-yellow-300">{$t('lpTrade.quoteStale')}</p>
  {/if}

  <div class="mt-4">
    <QuoteStatus
      loading={pairState.quoteStatus.state === 'loading'}
      error={pairState.quoteStatus.state === 'error' ? pairState.quoteStatus.message : ''}
      stale={quoteIsStale}
      source={pairState.quoteStatus.source}
      message={quoteIsStale ? $t('lpTrade.quoteStale') : pairState.quoteStatus.message}
    />
  </div>

  {#if quote?.quote}
    <div class="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
      <div class="rounded-lg bg-surface-dark p-3">
        <div class="text-xs text-gray-500">{$t('lpTrade.amountOut')}</div>
        <div class="mt-1 font-mono text-sm text-gray-100">{formatAsset(quote.quote.amount_out, quote.quote.asset_out)}</div>
      </div>
      <div class="rounded-lg bg-surface-dark p-3">
        <div class="text-xs text-gray-500">{$t('lpTrade.minimumOut')}</div>
        <div class="mt-1 font-mono text-sm text-gray-100">{formatAsset(quote.quote.minimum_out, quote.quote.asset_out)}</div>
      </div>
      <div class="rounded-lg bg-surface-dark p-3">
        <div class="text-xs text-gray-500">{$t('lpTrade.fee')}</div>
        <div class="mt-1 font-mono text-sm text-gray-100">{quote.quote.fee_bps} bps / {formatAsset(quote.quote.fee_amount_estimate, quote.quote.asset_in)}</div>
      </div>
      <div class="rounded-lg bg-surface-dark p-3">
        <div class="text-xs text-gray-500">{$t('lpTrade.priceImpact')}</div>
        <div class="mt-1 font-mono text-sm text-gray-100">{fmt(quote.quote.price_impact_bps)} bps</div>
      </div>
    </div>
  {/if}

  {#if swapPreview?.preview}
    <div class="mt-4 rounded-lg bg-surface-dark p-3 text-sm">
      <div class="text-gray-400">{$t('lpTrade.swapPreview')}</div>
      <div class="mt-1 font-mono text-sm text-gray-100">
        {formatAsset(swapPreview.preview.amount_in, swapPreview.preview.asset_in)} → {formatAsset(swapPreview.preview.amount_out, swapPreview.preview.asset_out)}
      </div>
      <div class="mt-2 text-xs text-gray-500">
        {$t('lpTrade.minimumOut')}: {formatAsset(swapPreview.preview.minimum_out, swapPreview.preview.asset_out)} · {$t('lpTrade.totalFee')}: {swapPreview.preview.total_fee}
      </div>
      <label class="mt-3 block max-w-xs">
        <span class="label">{$t('transfer.pin')}</span>
        <input class="input mt-1" type="password" bind:value={swapPin} autocomplete="current-password" />
      </label>
      <button class="btn-primary mt-3" type="button" on:click={submitSwapExecution} disabled={loadingSwapSubmit}>
        {loadingSwapSubmit ? $t('lpTrade.submitting') : $t('lpTrade.confirmSwap')}
      </button>
    </div>
  {/if}

  {#if swapResult}
    <p class="mt-3 text-sm text-green-300">{$t('lpTrade.swapSuccess')}</p>
  {/if}
</section>
