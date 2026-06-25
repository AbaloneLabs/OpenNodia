<script>
  import { t } from '../../i18n/index.js';

  export let pool = null;
  export let removeBurnLp = '';
  export let loadingRemovePrepare = false;
  export let currentWallet = null;
  export let selectedPoolCanRemoveLiquidity = false;
  export let removePreview = null;
  export let removePin = '';
  export let loadingRemoveSubmit = false;
  export let removeResult = null;
  export let prepareRemoveLiquidity = () => {};
  export let submitRemoveLiquidity = () => {};
  export let formatAsset = (raw) => String(raw ?? '0');
</script>

<div class="card">
  <h3 class="text-lg font-semibold text-gray-100">{$t('lpTrade.removeLiquidity')}</h3>
  <p class="mt-2 text-sm text-gray-500">{$t('lpTrade.removeLiquidityHint')}</p>
  <label class="mt-4 block">
    <span class="label">{$t('lpTrade.burnLp')}</span>
    <input class="input mt-1" bind:value={removeBurnLp} inputmode="numeric" placeholder="1000" />
  </label>
  <button
    class="btn-secondary mt-4"
    type="button"
    on:click={prepareRemoveLiquidity}
    disabled={loadingRemovePrepare || !currentWallet || !selectedPoolCanRemoveLiquidity || !pool?.pool?.lp_asset_id || pool.pool.total_lp_supply === 0}
  >
    {loadingRemovePrepare ? $t('lpTrade.preparing') : $t('lpTrade.prepareRemove')}
  </button>
  {#if removePreview?.preview}
    <div class="mt-4 rounded-lg bg-surface-dark p-3 text-sm">
      <div class="text-gray-400">{$t('lpTrade.expectedReceive')}</div>
      <div class="mt-1 font-mono text-sm text-gray-100">
        {formatAsset(removePreview.preview.amount_0, pool.pool.asset_0)} / {formatAsset(removePreview.preview.amount_1, pool.pool.asset_1)}
      </div>
      <div class="mt-2 text-xs text-gray-500">
        {$t('lpTrade.minimum')}: {formatAsset(removePreview.preview.minimum_0, pool.pool.asset_0)} / {formatAsset(removePreview.preview.minimum_1, pool.pool.asset_1)}
      </div>
      <label class="mt-3 block">
        <span class="label">{$t('transfer.pin')}</span>
        <input class="input mt-1" type="password" bind:value={removePin} autocomplete="current-password" />
      </label>
      <button class="btn-primary mt-3" type="button" on:click={submitRemoveLiquidity} disabled={loadingRemoveSubmit}>
        {loadingRemoveSubmit ? $t('lpTrade.submitting') : $t('lpTrade.confirmRemove')}
      </button>
    </div>
  {/if}
  {#if removeResult}
    <p class="mt-3 text-sm text-green-300">{$t('lpTrade.removeSuccess')}</p>
  {/if}
</div>
