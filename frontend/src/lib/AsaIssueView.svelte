<script>
  import { api, getToken } from '../api.js';
  import {
    formatRawAmount,
    parseDecimalToRaw,
    rawToSafeNumber,
    utf8ByteLength,
  } from '../amount.js';
  import { t } from '../i18n/index.js';
  import { activeView } from '../nav.js';
  import { activeWallet } from '../walletStore.js';

  let assetName = '';
  let unitName = '';
  let totalSupply = '';
  let decimals = '6';
  let url = '';
  let metadataHashB64 = '';
  let lockAuthorities = true;
  let defaultFrozen = false;
  let manager = '';
  let reserve = '';
  let freeze = '';
  let clawback = '';

  let step = 'form';
  let error = '';
  let preview = null;
  let preparedRequest = null;
  let pin = '';
  let result = null;
  let issuedAssets = [];
  let historyLoading = false;
  let lastHistoryWalletId = '';
  let configAssetId = '';
  let configManager = '';
  let configReserve = '';
  let configFreeze = '';
  let configClawback = '';
  let configPreview = null;
  let configLoading = false;

  $: currentWallet = $activeWallet;
  $: creator = currentWallet?.first_address || '';
  $: parsedDecimals = parseDecimalsValue(decimals);
  $: rawSupplyLabel = rawSupplyPreview();
  $: assetNameBytes = utf8ByteLength(assetName);
  $: unitNameBytes = utf8ByteLength(unitName);
  $: urlBytes = utf8ByteLength(url);
  $: clientMetadataWarning = url.trim() && !metadataHashB64.trim();
  $: if (lockAuthorities && defaultFrozen) defaultFrozen = false;
  $: if (currentWallet?.id && currentWallet.id !== lastHistoryWalletId) {
    lastHistoryWalletId = currentWallet.id;
    loadIssuedAssets();
  }

  function parseDecimalsValue(value) {
    const text = String(value ?? '').trim();
    if (!/^\d+$/.test(text)) return null;
    const parsed = Number(text);
    if (!Number.isInteger(parsed) || parsed < 0 || parsed > 19) return null;
    return parsed;
  }

  function rawSupplyPreview() {
    try {
      if (!totalSupply || parsedDecimals == null) return '0';
      return parseDecimalToRaw(totalSupply, parsedDecimals).toString();
    } catch (e) {
      return '—';
    }
  }

  function resetResultState() {
    error = '';
    preview = null;
    preparedRequest = null;
    result = null;
    pin = '';
  }

  function onAuthorityLockChange() {
    if (!lockAuthorities) return;
    defaultFrozen = false;
    manager = '';
    reserve = '';
    freeze = '';
    clawback = '';
  }

  function setCreatorAuthorities() {
    if (!creator) return;
    manager = creator;
    reserve = creator;
    freeze = creator;
    clawback = '';
  }

  function validateForm() {
    if (!currentWallet || !creator) throw new Error($t('asaIssue.noWalletSelected'));
    if (!assetName.trim()) throw new Error($t('asaIssue.assetNameRequired'));
    if (!unitName.trim()) throw new Error($t('asaIssue.unitNameRequired'));
    if (assetNameBytes > 32) throw new Error($t('asaIssue.assetNameTooLong'));
    if (unitNameBytes > 8) throw new Error($t('asaIssue.unitNameTooLong'));
    if (urlBytes > 96) throw new Error($t('asaIssue.urlTooLong'));
    if (parsedDecimals == null) throw new Error($t('asaIssue.invalidDecimals'));
    if (defaultFrozen && !freeze.trim()) throw new Error($t('asaIssue.freezeRequired'));

    const rawTotal = parseDecimalToRaw(totalSupply, parsedDecimals);
    return {
      creator,
      total: rawToSafeNumber(rawTotal),
      decimals: parsedDecimals,
      unitName: unitName.trim(),
      assetName: assetName.trim(),
      url: url.trim(),
      metadataHashB64: metadataHashB64.trim(),
      defaultFrozen,
      manager: lockAuthorities ? '' : manager.trim(),
      reserve: lockAuthorities ? '' : reserve.trim(),
      freeze: lockAuthorities ? '' : freeze.trim(),
      clawback: lockAuthorities ? '' : clawback.trim(),
      allowManagedAuthorities: !lockAuthorities,
    };
  }

  async function prepare() {
    resetResultState();
    try {
      preparedRequest = validateForm();
      step = 'preparing';
      preview = await api.prepareAssetCreate({
        walletId: currentWallet.id,
        ...preparedRequest,
      });
      step = 'review';
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
    if (!getToken()) {
      error = $t('common.sessionExpired');
      return;
    }
    if (!pin) {
      error = $t('asaIssue.pinRequired');
      return;
    }
    if (!preparedRequest) {
      error = $t('asaIssue.prepareRequired');
      return;
    }
    error = '';
    step = 'submitting';
    try {
      result = await api.createAsset({
        walletId: currentWallet.id,
        pin,
        intentId: preview?.intent_id,
        ...preparedRequest,
      });
      configAssetId = String(result.asset_id);
      pin = '';
      step = 'success';
      await loadIssuedAssets();
    } catch (e) {
      error = e.message;
      step = 'review';
    }
  }

  async function copyTxId() {
    if (!result?.txid) return;
    await navigator.clipboard.writeText(result.txid);
  }

  function formatAlgo(value) {
    return `${formatRawAmount(BigInt(value || 0), 6)} ALGO`;
  }

  function authorityText(value) {
    return value || $t('asaIssue.none');
  }

  async function loadIssuedAssets() {
    if (!currentWallet?.id) return;
    historyLoading = true;
    try {
      const resp = await api.listIssuedAssets(currentWallet.id);
      issuedAssets = resp.assets || [];
    } catch (_) {
      issuedAssets = [];
    } finally {
      historyLoading = false;
    }
  }

  function policyClass(policy) {
    if (policy?.grade === 'open') return 'border-green-500/30 bg-green-500/10 text-green-300';
    if (policy?.grade === 'regulated') return 'border-red-500/30 bg-red-500/10 text-red-300';
    return 'border-yellow-500/30 bg-yellow-500/10 text-yellow-300';
  }

  function eligibilityText(policy) {
    if (!policy) return '—';
    return policy.dex_eligible && policy.lp_eligible
      ? $t('asaIssue.dexLpEligible')
      : $t('asaIssue.dexLpBlocked');
  }

  function fillConfigCreatorAuthorities() {
    if (!creator) return;
    configManager = creator;
    configReserve = creator;
    configFreeze = creator;
    configClawback = '';
  }

  function clearConfigAuthorities() {
    configManager = '';
    configReserve = '';
    configFreeze = '';
    configClawback = '';
  }

  async function prepareConfigPreview() {
    if (!currentWallet || !creator) {
      error = $t('asaIssue.noWalletSelected');
      return;
    }
    const assetId = Number(configAssetId);
    if (!Number.isSafeInteger(assetId) || assetId <= 0) {
      error = $t('asaIssue.assetIdRequired');
      return;
    }
    configLoading = true;
    configPreview = null;
    error = '';
    try {
      configPreview = await api.prepareAssetConfig({
        walletId: currentWallet.id,
        signer: creator,
        assetId,
        manager: configManager.trim(),
        reserve: configReserve.trim(),
        freeze: configFreeze.trim(),
        clawback: configClawback.trim(),
      });
    } catch (e) {
      error = e.message;
    } finally {
      configLoading = false;
    }
  }
</script>

<div class="space-y-6">
  <div class="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
    <div>
      <h2 class="text-2xl font-bold text-gray-100">{$t('asaIssue.title')}</h2>
      <p class="mt-1 text-sm text-gray-500">{$t('asaIssue.subtitle')}</p>
    </div>
    <button class="btn-secondary text-sm" type="button" on:click={() => activeView.set('assets')}>
      {$t('asaIssue.openAssets')}
    </button>
  </div>

  {#if !currentWallet}
    <div class="card text-center">
      <p class="text-gray-400">{$t('asaIssue.noWalletSelected')}</p>
      <button class="btn-primary mt-4" type="button" on:click={() => activeView.set('wallets')}>
        {$t('assets.goToWallets')}
      </button>
    </div>
  {:else}
    <div class="grid gap-6 lg:grid-cols-[minmax(0,1fr)_22rem]">
      <section class="card">
        <div class="mb-5">
          <h3 class="text-lg font-semibold text-gray-100">{$t('asaIssue.formTitle')}</h3>
          <p class="mt-1 break-all font-mono text-xs text-gray-500">{creator}</p>
        </div>

        {#if step === 'form' || step === 'preparing'}
          <div class="space-y-5">
            <div class="grid gap-4 sm:grid-cols-2">
              <div>
                <div class="mb-1.5 flex items-center justify-between">
                  <label class="label mb-0" for="asa-name">{$t('asaIssue.assetName')}</label>
                  <span class="text-xs {assetNameBytes > 32 ? 'text-red-400' : 'text-gray-600'}">
                    {assetNameBytes}/32
                  </span>
                </div>
                <input
                  id="asa-name"
                  class="input"
                  bind:value={assetName}
                  placeholder={$t('asaIssue.assetNamePlaceholder')}
                  disabled={step === 'preparing'}
                />
              </div>

              <div>
                <div class="mb-1.5 flex items-center justify-between">
                  <label class="label mb-0" for="asa-unit">{$t('asaIssue.unitName')}</label>
                  <span class="text-xs {unitNameBytes > 8 ? 'text-red-400' : 'text-gray-600'}">
                    {unitNameBytes}/8
                  </span>
                </div>
                <input
                  id="asa-unit"
                  class="input uppercase"
                  bind:value={unitName}
                  placeholder={$t('asaIssue.unitNamePlaceholder')}
                  disabled={step === 'preparing'}
                />
              </div>
            </div>

            <div class="grid gap-4 sm:grid-cols-[minmax(0,1fr)_9rem]">
              <div>
                <label class="label" for="asa-total">{$t('asaIssue.totalSupply')}</label>
                <input
                  id="asa-total"
                  class="input"
                  inputmode="decimal"
                  bind:value={totalSupply}
                  placeholder="1000000"
                  disabled={step === 'preparing'}
                />
                <p class="mt-1 text-xs text-gray-600">
                  {$t('asaIssue.rawTotal')}: <span class="font-mono text-gray-400">{rawSupplyLabel}</span>
                </p>
              </div>
              <div>
                <label class="label" for="asa-decimals">{$t('asaIssue.decimals')}</label>
                <input
                  id="asa-decimals"
                  class="input"
                  inputmode="numeric"
                  bind:value={decimals}
                  disabled={step === 'preparing'}
                />
              </div>
            </div>

            <div class="border-t border-gray-800 pt-4">
              <h4 class="mb-3 text-sm font-semibold text-gray-200">{$t('asaIssue.metadataSection')}</h4>
              <div class="mb-1.5 flex items-center justify-between">
                <label class="label mb-0" for="asa-url">{$t('asaIssue.url')}</label>
                <span class="text-xs {urlBytes > 96 ? 'text-red-400' : 'text-gray-600'}">
                  {urlBytes}/96
                </span>
              </div>
              <input
                id="asa-url"
                class="input"
                bind:value={url}
                placeholder="https://example.com/asset.json"
                disabled={step === 'preparing'}
              />
              {#if clientMetadataWarning}
                <p class="mt-2 rounded-lg border border-yellow-500/30 bg-yellow-500/10 px-3 py-2 text-xs text-yellow-300">
                  {$t('asaIssue.urlWithoutHashWarning')}
                </p>
              {/if}
            </div>

            <div>
              <label class="label" for="asa-metadata">{$t('asaIssue.metadataHash')}</label>
              <input
                id="asa-metadata"
                class="input font-mono text-sm"
                bind:value={metadataHashB64}
                placeholder={$t('asaIssue.metadataHashPlaceholder')}
                disabled={step === 'preparing'}
              />
              <p class="mt-1 text-xs text-gray-600">{$t('asaIssue.metadataHashHint')}</p>
            </div>

            <div class="rounded-xl border border-gray-700 bg-gray-900/40 p-4">
              <h4 class="mb-3 text-sm font-semibold text-gray-200">{$t('asaIssue.authoritySection')}</h4>
              <label class="flex items-start gap-3">
                <input
                  class="mt-1"
                  type="checkbox"
                  bind:checked={lockAuthorities}
                  on:change={onAuthorityLockChange}
                  disabled={step === 'preparing'}
                />
                <span>
                  <span class="block text-sm font-medium text-gray-200">{$t('asaIssue.lockAuthorities')}</span>
                  <span class="mt-1 block text-xs text-gray-500">{$t('asaIssue.lockAuthoritiesHint')}</span>
                </span>
              </label>

              {#if !lockAuthorities}
                <div class="mt-4 space-y-4">
                  <button class="btn-secondary text-xs" type="button" on:click={setCreatorAuthorities}>
                    {$t('asaIssue.useCreatorAuthorities')}
                  </button>

                  <div class="grid gap-3 sm:grid-cols-2">
                    <div>
                      <label class="label" for="asa-manager">{$t('asaIssue.manager')}</label>
                      <input id="asa-manager" class="input font-mono text-xs" bind:value={manager} />
                      <p class="mt-1 text-[11px] text-gray-600">{$t('asaIssue.managerHint')}</p>
                    </div>
                    <div>
                      <label class="label" for="asa-reserve">{$t('asaIssue.reserve')}</label>
                      <input id="asa-reserve" class="input font-mono text-xs" bind:value={reserve} />
                      <p class="mt-1 text-[11px] text-gray-600">{$t('asaIssue.reserveHint')}</p>
                    </div>
                    <div>
                      <label class="label" for="asa-freeze">{$t('asaIssue.freeze')}</label>
                      <input id="asa-freeze" class="input font-mono text-xs" bind:value={freeze} />
                      <p class="mt-1 text-[11px] text-gray-600">{$t('asaIssue.freezeHint')}</p>
                    </div>
                    <div>
                      <label class="label" for="asa-clawback">{$t('asaIssue.clawback')}</label>
                      <input id="asa-clawback" class="input font-mono text-xs" bind:value={clawback} />
                      <p class="mt-1 text-[11px] text-gray-600">{$t('asaIssue.clawbackHint')}</p>
                    </div>
                  </div>

                  <label class="flex items-center gap-2 text-sm text-gray-300">
                    <input type="checkbox" bind:checked={defaultFrozen} />
                    {$t('asaIssue.defaultFrozen')}
                  </label>
                </div>
              {/if}
            </div>

            {#if error}
              <div class="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-400">
                {error}
              </div>
            {/if}

            <button class="btn-primary w-full" type="button" on:click={prepare} disabled={step === 'preparing'}>
              {step === 'preparing' ? $t('asaIssue.preparing') : $t('asaIssue.review')}
            </button>
          </div>
        {:else if step === 'review' || step === 'submitting'}
          <div class="space-y-5">
            <div class="rounded-xl border border-gray-700 bg-gray-900/50 p-4">
              <h4 class="mb-3 text-sm font-medium text-gray-200">{$t('asaIssue.reviewTitle')}</h4>
              <dl class="space-y-2 text-sm">
                <div class="flex justify-between gap-4">
                  <dt class="text-gray-500">{$t('asaIssue.assetName')}</dt>
                  <dd class="text-right text-gray-200">{preview.preview.asset_name}</dd>
                </div>
                <div class="flex justify-between gap-4">
                  <dt class="text-gray-500">{$t('asaIssue.unitName')}</dt>
                  <dd class="font-mono text-gray-200">{preview.preview.unit_name}</dd>
                </div>
                <div class="flex justify-between gap-4">
                  <dt class="text-gray-500">{$t('asaIssue.totalSupply')}</dt>
                  <dd class="font-mono text-gray-200">
                    {formatRawAmount(BigInt(preview.preview.total), preview.preview.decimals)}
                  </dd>
                </div>
                <div class="flex justify-between gap-4">
                  <dt class="text-gray-500">{$t('asaIssue.rawTotal')}</dt>
                  <dd class="font-mono text-gray-200">{preview.preview.total}</dd>
                </div>
                <div class="flex justify-between gap-4">
                  <dt class="text-gray-500">{$t('transfer.fee')}</dt>
                  <dd class="font-mono text-gray-200">{formatAlgo(preview.preview.fee)}</dd>
                </div>
                <div class="flex justify-between gap-4">
                  <dt class="text-gray-500">{$t('asaIssue.requiredBalance')}</dt>
                  <dd class="font-mono text-gray-200">{formatAlgo(preview.preview.required_balance)}</dd>
                </div>
                <div class="flex justify-between gap-4">
                  <dt class="text-gray-500">{$t('asaIssue.requiredMinBalance')}</dt>
                  <dd class="font-mono text-gray-200">{formatAlgo(preview.preview.required_min_balance)}</dd>
                </div>
                <div>
                  <dt class="mb-1 text-gray-500">{$t('asaIssue.creator')}</dt>
                  <dd class="break-all font-mono text-xs text-gray-300">{preview.preview.creator}</dd>
                </div>
              </dl>
            </div>

            <div class="rounded-xl border border-gray-700 bg-gray-900/50 p-4">
              <h4 class="mb-3 text-sm font-medium text-gray-200">{$t('asaIssue.metadataSection')}</h4>
              <div class="grid gap-2 text-sm sm:grid-cols-2">
                <div class="rounded-lg bg-surface-dark p-3">
                  <div class="text-xs text-gray-500">ARC-3</div>
                  <div class="mt-1 text-gray-200">
                    {preview.metadata.arc3_marker_present ? '#arc3' : $t('asaIssue.arc3MarkerMissing')}
                  </div>
                </div>
                <div class="rounded-lg bg-surface-dark p-3">
                  <div class="text-xs text-gray-500">{$t('asaIssue.remoteMetadata')}</div>
                  <div class="mt-1 text-gray-200">
                    {preview.metadata.hash_verified ? $t('asaIssue.hashVerified') : $t('asaIssue.notVerified')}
                  </div>
                </div>
              </div>
              {#if preview.metadata.warnings?.length}
                <ul class="mt-3 list-disc space-y-1 pl-5 text-xs text-yellow-300">
                  {#each preview.metadata.warnings as warning}
                    <li>{warning}</li>
                  {/each}
                </ul>
              {/if}
            </div>

            <div class="rounded-xl border p-4 {policyClass(preview.policy)}">
              <h4 class="mb-2 text-sm font-medium">{$t('asaIssue.policySnapshot')}</h4>
              <div class="text-sm">
                <span class="uppercase">{preview.policy.grade}</span> · {eligibilityText(preview.policy)}
              </div>
              {#if preview.policy.warnings?.length}
                <ul class="mt-2 list-disc space-y-1 pl-5 text-xs">
                  {#each preview.policy.warnings as warning}
                    <li>{warning}</li>
                  {/each}
                </ul>
              {/if}
            </div>

            <div class="rounded-xl border border-gray-700 bg-gray-900/50 p-4">
              <h4 class="mb-3 text-sm font-medium text-gray-200">{$t('asaIssue.authorities')}</h4>
              <dl class="space-y-2 text-sm">
                <div>
                  <dt class="mb-1 text-gray-500">{$t('asaIssue.manager')}</dt>
                  <dd class="break-all font-mono text-xs text-gray-300">{authorityText(preview.preview.manager)}</dd>
                </div>
                <div>
                  <dt class="mb-1 text-gray-500">{$t('asaIssue.reserve')}</dt>
                  <dd class="break-all font-mono text-xs text-gray-300">{authorityText(preview.preview.reserve)}</dd>
                </div>
                <div>
                  <dt class="mb-1 text-gray-500">{$t('asaIssue.freeze')}</dt>
                  <dd class="break-all font-mono text-xs text-gray-300">{authorityText(preview.preview.freeze)}</dd>
                </div>
                <div>
                  <dt class="mb-1 text-gray-500">{$t('asaIssue.clawback')}</dt>
                  <dd class="break-all font-mono text-xs text-gray-300">{authorityText(preview.preview.clawback)}</dd>
                </div>
              </dl>
            </div>

            <div>
              <label class="label" for="asa-pin">{$t('transfer.pin')}</label>
              <input
                id="asa-pin"
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
              <button class="btn-secondary flex-1" type="button" on:click={back} disabled={step === 'submitting'}>
                {$t('common.back')}
              </button>
              <button class="btn-primary flex-1" type="button" on:click={submit} disabled={step === 'submitting'}>
                {step === 'submitting' ? $t('asaIssue.submitting') : $t('asaIssue.confirmCreate')}
              </button>
            </div>
          </div>
        {:else if step === 'success'}
          <div class="space-y-5 text-center">
            <div class="mx-auto flex h-14 w-14 items-center justify-center rounded-full bg-green-500/10 text-green-400">
              <svg class="h-7 w-7" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" d="m5 13 4 4L19 7" />
              </svg>
            </div>
            <div>
              <h3 class="text-lg font-semibold text-gray-100">{$t('asaIssue.success')}</h3>
              <p class="mt-1 text-sm text-gray-500">
                {$t('transfer.confirmedRound', { round: result.confirmed_round })}
              </p>
            </div>
            <div class="rounded-lg border border-gray-700 bg-gray-900/50 p-4 text-left text-sm">
              <div class="mb-2 flex justify-between gap-4">
                <span class="text-gray-500">{$t('asaIssue.assetId')}</span>
                <span class="font-mono text-gray-200">{result.asset_id}</span>
              </div>
              <div class="mb-2 flex justify-between gap-4">
                <span class="text-gray-500">{$t('asaIssue.policySnapshot')}</span>
                <span class="font-mono text-gray-200">{result.policy.grade}</span>
              </div>
              <div class="mb-2 flex justify-between gap-4">
                <span class="text-gray-500">{$t('asaIssue.dexLpEligibility')}</span>
                <span class="text-gray-200">{eligibilityText(result.policy)}</span>
              </div>
              <div class="mb-2 flex justify-between gap-4">
                <span class="text-gray-500">{$t('asaIssue.balanceChange')}</span>
                <span class="font-mono text-gray-200">{formatAlgo(result.balance_before)} → {formatAlgo(result.balance_after)}</span>
              </div>
              <div class="mb-3 flex justify-between gap-4">
                <span class="text-gray-500">{$t('asaIssue.minBalanceChange')}</span>
                <span class="font-mono text-gray-200">{formatAlgo(result.min_balance_before)} → {formatAlgo(result.min_balance_after)}</span>
              </div>
              <button class="block w-full break-all text-left font-mono text-xs text-algo-400" type="button" on:click={copyTxId}>
                {result.txid}
              </button>
            </div>
            <div class="flex gap-3">
              <button class="btn-secondary flex-1" type="button" on:click={() => activeView.set('assets')}>
                {$t('asaIssue.openAssets')}
              </button>
              <button class="btn-primary flex-1" type="button" on:click={() => { step = 'form'; resetResultState(); }}>
                {$t('asaIssue.createAnother')}
              </button>
            </div>
          </div>
        {/if}
      </section>

      <aside class="space-y-4">
        <div class="card">
          <h3 class="text-sm font-semibold text-gray-100">{$t('asaIssue.policyTitle')}</h3>
          <p class="mt-2 text-sm text-gray-500">{$t('asaIssue.policyHint')}</p>
        </div>
        <div class="card">
          <h3 class="text-sm font-semibold text-gray-100">{$t('asaIssue.costTitle')}</h3>
          <p class="mt-2 text-sm text-gray-500">{$t('asaIssue.costHint')}</p>
        </div>
        <div class="card">
          <div class="flex items-center justify-between gap-3">
            <h3 class="text-sm font-semibold text-gray-100">{$t('asaIssue.historyTitle')}</h3>
            <button class="text-xs text-gray-500 underline hover:text-gray-300" type="button" on:click={loadIssuedAssets}>
              {$t('common.refresh')}
            </button>
          </div>
          {#if historyLoading}
            <p class="mt-3 text-sm text-gray-500">{$t('common.loading')}</p>
          {:else if issuedAssets.length === 0}
            <p class="mt-3 text-sm text-gray-500">{$t('asaIssue.noHistory')}</p>
          {:else}
            <div class="mt-3 space-y-2">
              {#each issuedAssets.slice(0, 5) as asset}
                <div class="rounded-lg border border-gray-700 bg-surface-dark p-3 text-xs">
                  <div class="flex items-center justify-between gap-2">
                    <span class="font-mono text-gray-100">#{asset.asset_id}</span>
                    <span class="rounded-full px-2 py-0.5 {asset.policy_grade === 'open' ? 'bg-green-500/10 text-green-300' : 'bg-red-500/10 text-red-300'}">
                      {asset.policy_grade}
                    </span>
                  </div>
                  <div class="mt-1 truncate font-mono text-gray-500">{asset.txid}</div>
                  <div class="mt-1 text-gray-500">{$t('transfer.confirmedRound', { round: asset.confirmed_round })}</div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
        <div class="card">
          <h3 class="text-sm font-semibold text-gray-100">{$t('asaIssue.configPreviewTitle')}</h3>
          <label class="mt-4 block">
            <span class="label">{$t('asaIssue.assetId')}</span>
            <input class="input mt-1" bind:value={configAssetId} inputmode="numeric" placeholder="ASA ID" />
          </label>
          <div class="mt-3 flex flex-wrap gap-2">
            <button class="btn-secondary text-xs" type="button" on:click={fillConfigCreatorAuthorities}>
              {$t('asaIssue.useCreatorAuthorities')}
            </button>
            <button class="btn-secondary text-xs" type="button" on:click={clearConfigAuthorities}>
              {$t('asaIssue.lockAllAuthorities')}
            </button>
          </div>
          <div class="mt-3 space-y-3">
            <label class="block">
              <span class="label">{$t('asaIssue.manager')}</span>
              <input class="input mt-1 font-mono text-xs" bind:value={configManager} />
            </label>
            <label class="block">
              <span class="label">{$t('asaIssue.reserve')}</span>
              <input class="input mt-1 font-mono text-xs" bind:value={configReserve} />
            </label>
            <label class="block">
              <span class="label">{$t('asaIssue.freeze')}</span>
              <input class="input mt-1 font-mono text-xs" bind:value={configFreeze} />
            </label>
            <label class="block">
              <span class="label">{$t('asaIssue.clawback')}</span>
              <input class="input mt-1 font-mono text-xs" bind:value={configClawback} />
            </label>
          </div>
          <button class="btn-secondary mt-4 w-full" type="button" on:click={prepareConfigPreview} disabled={configLoading}>
            {configLoading ? $t('asaIssue.preparing') : $t('asaIssue.prepareConfigPreview')}
          </button>
          {#if configPreview?.preview}
            <div class="mt-4 rounded-lg border border-gray-700 bg-surface-dark p-3 text-xs">
              <div class="font-semibold text-gray-200">
                {configPreview.preview.current_policy.grade} → {configPreview.preview.next_policy.grade}
              </div>
              <div class="mt-1 text-gray-500">
                {$t('transfer.fee')}: {formatAlgo(configPreview.preview.fee)}
              </div>
              <div class="mt-2 break-all font-mono text-gray-400">{configPreview.tx_hash}</div>
              {#if configPreview.preview.warnings?.length}
                <ul class="mt-2 list-disc space-y-1 pl-5 text-yellow-300">
                  {#each configPreview.preview.warnings as warning}
                    <li>{warning}</li>
                  {/each}
                </ul>
              {/if}
            </div>
          {/if}
        </div>
      </aside>
    </div>
  {/if}
</div>
