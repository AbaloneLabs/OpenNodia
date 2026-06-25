<script>
  import { t } from '../../i18n/index.js';

  export let appId = '';
  export let loadingPool = false;
  export let loadPool = () => {};
</script>

<div class="card">
  <h3 class="text-lg font-semibold text-gray-100">{$t('lpTrade.loadByApp')}</h3>
  <p class="mt-2 text-sm text-gray-500">{$t('lpTrade.loadByAppHint')}</p>

  <label class="mt-4 block">
    <span class="label">{$t('lpTrade.appId')}</span>
    <input class="input mt-1" bind:value={appId} inputmode="numeric" placeholder="123456" />
  </label>
  <button class="btn-primary mt-4" type="button" on:click={() => loadPool()} disabled={loadingPool}>
    {loadingPool ? $t('lpTrade.loadingPool') : $t('lpTrade.loadPool')}
  </button>
</div>
