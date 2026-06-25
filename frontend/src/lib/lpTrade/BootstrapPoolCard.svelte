<script>
  import { t } from '../../i18n/index.js';

  export let pool = null;
  export let bootstrapAmount0 = '';
  export let bootstrapAmount1 = '';
  export let loadingBootstrapPrepare = false;
  export let currentWallet = null;
  export let selectedPoolIsNative = false;
  export let bootstrapPreview = null;
  export let bootstrapPin = '';
  export let loadingBootstrapSubmit = false;
  export let bootstrapResult = null;
  export let prepareBootstrapPool = () => {};
  export let submitBootstrapPool = () => {};
  export let formatAsset = (raw) => String(raw ?? '0');
  export let formatLp = (raw) => String(raw ?? '0');
</script>

<div class="card">
  <h3 class="text-lg font-semibold text-gray-100">{$t('lpTrade.bootstrapPool')}</h3>
  <p class="mt-2 text-sm text-gray-500">{$t('lpTrade.bootstrapPoolHint')}</p>
  <div class="mt-4 grid gap-3 sm:grid-cols-2">
    <label class="block">
      <span class="label">{$t('lpTrade.amount0')}</span>
      <input class="input mt-1" bind:value={bootstrapAmount0} inputmode="numeric" placeholder="1000000" />
    </label>
    <label class="block">
      <span class="label">{$t('lpTrade.amount1')}</span>
      <input class="input mt-1" bind:value={bootstrapAmount1} inputmode="numeric" placeholder="1000000" />
    </label>
  </div>
  <button
    class="btn-secondary mt-4"
    type="button"
    on:click={prepareBootstrapPool}
    disabled={loadingBootstrapPrepare || !currentWallet || !selectedPoolIsNative || !pool?.pool?.lp_asset_id || pool.pool.total_lp_supply !== 0}
  >
    {loadingBootstrapPrepare ? $t('lpTrade.preparing') : $t('lpTrade.prepareBootstrap')}
  </button>
  {#if bootstrapPreview?.preview}
    <div class="mt-4 rounded-lg bg-surface-dark p-3 text-sm">
      <div class="text-gray-400">{$t('lpTrade.mintedLp')}</div>
      <div class="mt-1 font-mono text-sm text-gray-100">
        {formatLp(bootstrapPreview.preview.minted_lp)} / {$t('lpTrade.minimum')} {formatLp(bootstrapPreview.preview.minimum_lp)}
      </div>
      <div class="mt-2 text-xs text-gray-500">
        {formatAsset(bootstrapPreview.preview.amount_0, pool.pool.asset_0)} / {formatAsset(bootstrapPreview.preview.amount_1, pool.pool.asset_1)}
      </div>
      <div class="mt-2 text-xs text-gray-500">
        {$t('lpTrade.deadlineRound')}: {bootstrapPreview.preview.deadline_round} · {$t('lpTrade.totalFee')}: {bootstrapPreview.preview.total_fee}
      </div>
      {#if bootstrapPreview.preview.bootstrap}
        <div class="mt-2 rounded bg-gray-900/40 p-2 text-xs text-gray-400">
          <div>{$t('lpTrade.initialPrice')}: {bootstrapPreview.preview.bootstrap.initial_price_display}</div>
          <div>{$t('lpTrade.priceImpact')}: {bootstrapPreview.preview.bootstrap.price_impact_bps} bps</div>
          <div>{$t('lpTrade.minBalance')}: {bootstrapPreview.preview.bootstrap.provider_min_balance_microalgo} &micro;ALGO · {$t('lpTrade.available')}: {bootstrapPreview.preview.bootstrap.provider_available_microalgo} &micro;ALGO</div>
          <div>{$t('lpTrade.atomicGroup')}: {bootstrapPreview.preview.atomic_group_size} tx · {$t('lpTrade.networkFee')}: {bootstrapPreview.preview.bootstrap.network_fee_microalgo} &micro;ALGO</div>
        </div>
      {/if}
      <label class="mt-3 block">
        <span class="label">{$t('transfer.pin')}</span>
        <input class="input mt-1" type="password" bind:value={bootstrapPin} autocomplete="current-password" />
      </label>
      <button class="btn-primary mt-3" type="button" on:click={submitBootstrapPool} disabled={loadingBootstrapSubmit}>
        {loadingBootstrapSubmit ? $t('lpTrade.submitting') : $t('lpTrade.confirmBootstrap')}
      </button>
    </div>
  {/if}
  {#if bootstrapResult}
    <p class="mt-3 text-sm text-green-300">{$t('lpTrade.bootstrapSuccess')}</p>
  {/if}
</div>
