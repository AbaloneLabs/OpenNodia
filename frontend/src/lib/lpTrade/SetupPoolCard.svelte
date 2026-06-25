<script>
  import { t } from '../../i18n/index.js';

  export let setupAppId = '';
  export let setupFundingMicroalgo = 500000;
  export let loadingSetupPrepare = false;
  export let currentWallet = null;
  export let setupPreview = null;
  export let setupPin = '';
  export let loadingSetupSubmit = false;
  export let setupResult = null;
  export let prepareSetupPool = () => {};
  export let submitSetupPool = () => {};
</script>

<div class="card">
  <h3 class="text-lg font-semibold text-gray-100">{$t('lpTrade.setupPool')}</h3>
  <p class="mt-2 text-sm text-gray-500">{$t('lpTrade.setupPoolHint')}</p>
  <div class="mt-4 grid gap-3 sm:grid-cols-2">
    <label class="block">
      <span class="label">{$t('lpTrade.appId')}</span>
      <input class="input mt-1" bind:value={setupAppId} inputmode="numeric" placeholder="123456" />
    </label>
    <label class="block">
      <span class="label">{$t('lpTrade.fundingMicroalgo')}</span>
      <input class="input mt-1" bind:value={setupFundingMicroalgo} inputmode="numeric" />
    </label>
  </div>
  <button class="btn-secondary mt-4" type="button" on:click={prepareSetupPool} disabled={loadingSetupPrepare || !currentWallet}>
    {loadingSetupPrepare ? $t('lpTrade.preparing') : $t('lpTrade.prepareSetup')}
  </button>
  {#if setupPreview?.preview}
    <div class="mt-4 rounded-lg bg-surface-dark p-3 text-sm">
      <div class="text-gray-400">{$t('lpTrade.appAddress')}</div>
      <div class="mt-1 break-all font-mono text-xs text-gray-100">{setupPreview.preview.app_address}</div>
      <div class="mt-2 text-gray-500">
        {$t('lpTrade.setupCost', { funding: setupPreview.preview.funding_algo, fee: setupPreview.preview.setup_fee })}
      </div>
      <label class="mt-3 block">
        <span class="label">{$t('transfer.pin')}</span>
        <input class="input mt-1" type="password" bind:value={setupPin} autocomplete="current-password" />
      </label>
      <button class="btn-primary mt-3" type="button" on:click={submitSetupPool} disabled={loadingSetupSubmit}>
        {loadingSetupSubmit ? $t('lpTrade.submitting') : $t('lpTrade.confirmSetup')}
      </button>
    </div>
  {/if}
  {#if setupResult}
    <p class="mt-3 text-sm text-green-300">
      {$t('lpTrade.setupSuccess', { lpAsset: setupResult.pool.lp_asset_id })}
    </p>
  {/if}
</div>
