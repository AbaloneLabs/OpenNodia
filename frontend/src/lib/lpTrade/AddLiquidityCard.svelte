<script>
  import { t } from '../../i18n/index.js';

  export let pool = null;
  export let addDesired0 = '';
  export let addDesired1 = '';
  export let loadingAddPrepare = false;
  export let currentWallet = null;
  export let selectedPoolCanAddLiquidity = false;
  export let addPreview = null;
  export let addPin = '';
  export let loadingAddSubmit = false;
  export let addResult = null;
  export let prepareAddLiquidity = () => {};
  export let submitAddLiquidity = () => {};
  export let formatAsset = (raw) => String(raw ?? '0');
  export let formatLp = (raw) => String(raw ?? '0');
</script>

<div class="card">
  <h3 class="text-lg font-semibold text-gray-100">{$t('lpTrade.addLiquidity')}</h3>
  <p class="mt-2 text-sm text-gray-500">{$t('lpTrade.addLiquidityHint')}</p>
  <div class="mt-4 grid gap-3 sm:grid-cols-2">
    <label class="block">
      <span class="label">{$t('lpTrade.desired0')}</span>
      <input class="input mt-1" bind:value={addDesired0} inputmode="numeric" placeholder="1000000" />
    </label>
    <label class="block">
      <span class="label">{$t('lpTrade.desired1')}</span>
      <input class="input mt-1" bind:value={addDesired1} inputmode="numeric" placeholder="1000000" />
    </label>
  </div>
  <button
    class="btn-secondary mt-4"
    type="button"
    on:click={prepareAddLiquidity}
    disabled={loadingAddPrepare || !currentWallet || !selectedPoolCanAddLiquidity || !pool?.pool?.lp_asset_id || pool.pool.total_lp_supply === 0}
  >
    {loadingAddPrepare ? $t('lpTrade.preparing') : $t('lpTrade.prepareAdd')}
  </button>
  {#if addPreview?.preview}
    <div class="mt-4 rounded-lg bg-surface-dark p-3 text-sm">
      <div class="text-gray-400">{$t('lpTrade.actualDeposit')}</div>
      <div class="mt-1 font-mono text-sm text-gray-100">
        {formatAsset(addPreview.preview.amount_0, pool.pool.asset_0)} / {formatAsset(addPreview.preview.amount_1, pool.pool.asset_1)}
      </div>
      <div class="mt-2 text-xs text-gray-500">
        {$t('lpTrade.mintedLp')}: {formatLp(addPreview.preview.minted_lp)} · {$t('lpTrade.minimum')}: {formatLp(addPreview.preview.minimum_lp)}
      </div>
      <label class="mt-3 block">
        <span class="label">{$t('transfer.pin')}</span>
        <input class="input mt-1" type="password" bind:value={addPin} autocomplete="current-password" />
      </label>
      <button class="btn-primary mt-3" type="button" on:click={submitAddLiquidity} disabled={loadingAddSubmit}>
        {loadingAddSubmit ? $t('lpTrade.submitting') : $t('lpTrade.confirmAdd')}
      </button>
    </div>
  {/if}
  {#if addResult}
    <p class="mt-3 text-sm text-green-300">{$t('lpTrade.addSuccess')}</p>
  {/if}
</div>
