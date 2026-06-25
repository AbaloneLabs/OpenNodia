<script>
  import { t } from '../../i18n/index.js';
  import AssetSearch from '../AssetSearch.svelte';

  export let createAssetA = '0';
  export let createAssetB = '';
  export let createPairProfile = 'standard';
  export let createFeeBps = 30;
  export let createAcknowledgeNoOwner = false;
  export let loadingCreateLookup = false;
  export let createExistingPool = null;
  export let createExistingPools = [];
  export let createLookupNote = '';
  export let loadingCreatePrepare = false;
  export let currentWallet = null;
  export let createPreview = null;
  export let setupFundingMicroalgo = 500000;
  export let createPin = '';
  export let loadingCreateSubmit = false;
  export let createResult = null;
  export let selectedAssetFromInput = () => null;
  export let onCreateAssetASelect = () => {};
  export let onCreateAssetBSelect = () => {};
  export let switchCreatePair = () => {};
  export let applyCreatePairProfile = () => {};
  export let scheduleCreateLookup = () => {};
  export let choosePool = () => {};
  export let prepareCreatePool = () => {};
  export let submitCreatePool = () => {};
</script>

<div class="card">
  <h3 class="text-lg font-semibold text-gray-100">{$t('lpTrade.createPool')}</h3>
  <p class="mt-2 text-sm text-gray-500">{$t('lpTrade.createPoolHint')}</p>
  <p class="mt-2 text-sm text-gray-500">{$t('lpTrade.createFlowHint')}</p>
  <div class="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
    <div class="block">
      <span class="label">{$t('lpTrade.assetA')}</span>
      <div class="mt-1">
        <AssetSearch
          placeholder="0 (ALGO)"
          allowManualEntry={true}
          selectedAsset={selectedAssetFromInput(createAssetA)}
          on:select={(event) => onCreateAssetASelect(event.detail)}
        />
      </div>
    </div>
    <div class="flex items-end">
      <button class="btn-secondary w-full text-xs" type="button" on:click={switchCreatePair}>
        {$t('dex.swapPair')}
      </button>
    </div>
    <div class="block">
      <span class="label">{$t('lpTrade.assetB')}</span>
      <div class="mt-1">
        <AssetSearch
          placeholder="ASA ID / name"
          allowManualEntry={true}
          selectedAsset={selectedAssetFromInput(createAssetB)}
          on:select={(event) => onCreateAssetBSelect(event.detail)}
        />
      </div>
    </div>
    <label class="block">
      <span class="label">{$t('lpTrade.pairProfile')}</span>
      <select class="input mt-1" bind:value={createPairProfile} on:change={applyCreatePairProfile}>
        <option value="standard">{$t('lpTrade.pairProfileStandard')}</option>
        <option value="verifiedPeg">{$t('lpTrade.pairProfilePeg')}</option>
        <option value="volatile">{$t('lpTrade.pairProfileVolatile')}</option>
      </select>
    </label>
    <label class="block lg:col-start-4">
      <span class="label">{$t('lpTrade.feeBps')}</span>
      <select class="input mt-1" bind:value={createFeeBps} on:change={scheduleCreateLookup}>
        <option value={5}>5</option>
        <option value={30}>30</option>
        <option value={100}>100</option>
      </select>
    </label>
  </div>
  <p class="mt-2 text-xs text-gray-500">{$t('lpTrade.feeRecommendationHint')}</p>

  <div class="mt-4 rounded-lg border border-gray-700 bg-surface-dark p-3 text-sm">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <span class="text-gray-400">{$t('lpTrade.existingPools')}</span>
      {#if loadingCreateLookup}
        <span class="text-xs text-gray-500">{$t('lpTrade.createLookupLoading')}</span>
      {/if}
    </div>
    {#if createExistingPool}
      <div class="mt-2 rounded-lg border border-yellow-500/30 bg-yellow-500/10 p-3">
        <p class="text-yellow-300">{$t('lpTrade.existingPoolFound', { appId: createExistingPool.app_id })}</p>
        <button class="btn-secondary mt-3" type="button" on:click={() => choosePool(createExistingPool)}>
          {$t('lpTrade.existingPoolAction')}
        </button>
      </div>
    {:else}
      <p class="mt-2 text-gray-500">{createLookupNote || $t('lpTrade.noExistingPool')}</p>
    {/if}
    {#if createExistingPools.length > 0}
      <div class="mt-3 flex flex-wrap gap-2">
        {#each createExistingPools as item (item.app_id)}
          <button class="rounded-full bg-gray-700/60 px-3 py-1 text-xs text-gray-200" type="button" on:click={() => choosePool(item)}>
            App {item.app_id} · {item.fee_bps} bps
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <label class="mt-4 flex items-start gap-2 text-sm text-gray-300">
    <input class="mt-1" type="checkbox" bind:checked={createAcknowledgeNoOwner} />
    <span>{$t('lpTrade.creatorNoOwnerConfirm')}</span>
  </label>

  <button class="btn-secondary mt-4" type="button" on:click={prepareCreatePool} disabled={loadingCreatePrepare || !currentWallet || Boolean(createExistingPool)}>
    {loadingCreatePrepare ? $t('lpTrade.preparing') : $t('lpTrade.prepareCreate')}
  </button>
  {#if createPreview?.preview}
    <div class="mt-4 rounded-lg bg-surface-dark p-3 text-sm">
      <div class="text-gray-400">{$t('lpTrade.poolId')}</div>
      <div class="mt-1 break-all font-mono text-xs text-gray-100">{createPreview.preview.pool_id}</div>
      <div class="mt-2 text-gray-500">{createPreview.preview.asset_0} / {createPreview.preview.asset_1} · {createPreview.preview.fee_bps} bps</div>
      <p class="mt-2 text-xs {createPreview.preview.registered_on_create ? 'text-green-300' : 'text-yellow-300'}">
        {createPreview.preview.registered_on_create ? $t('lpTrade.registeredOnCreate') : $t('lpTrade.unregisteredOnCreate')}
      </p>
      {#if createPreview.txs?.length}
        <div class="mt-3 text-xs text-gray-500">
          {$t('lpTrade.txGroup')}: {createPreview.txs.length} · {$t('lpTrade.totalFee')}: {createPreview.preview.app_create_fee}
        </div>
      {/if}
      <div class="mt-3 rounded-lg border border-gray-700 bg-black/10 p-3">
        <div class="text-xs font-semibold uppercase tracking-wide text-gray-400">{$t('lpTrade.createLaunchChecklist')}</div>
        <p class="mt-2 text-xs text-gray-500">
          {$t('lpTrade.createLaunchChecklistHint', {
            funding: setupFundingMicroalgo,
            fee: createPreview.preview.app_create_fee,
          })}
        </p>
      </div>
      <label class="mt-3 block">
        <span class="label">{$t('transfer.pin')}</span>
        <input class="input mt-1" type="password" bind:value={createPin} autocomplete="current-password" />
      </label>
      <button class="btn-primary mt-3" type="button" on:click={submitCreatePool} disabled={loadingCreateSubmit}>
        {loadingCreateSubmit ? $t('lpTrade.submitting') : $t('lpTrade.confirmCreate')}
      </button>
    </div>
  {/if}
  {#if createResult}
    <p class="mt-3 text-sm text-green-300">
      {$t('lpTrade.createSuccess', { appId: createResult.app_id })}
    </p>
  {/if}
</div>
