<script>
  import { onMount } from 'svelte';
  import { api, getToken } from '../api.js';
  import { t } from '../i18n/index.js';
  import {
    setQuoteStatus,
    setTradeBaseAsset,
    setTradeQuoteAsset,
    swapTradePair,
    tradeState,
  } from '../tradeState.js';
  import { activeWallet } from '../walletStore.js';
  import AddLiquidityCard from './lpTrade/AddLiquidityCard.svelte';
  import BootstrapPoolCard from './lpTrade/BootstrapPoolCard.svelte';
  import CreatePoolCard from './lpTrade/CreatePoolCard.svelte';
  import DiscoverPoolsCard from './lpTrade/DiscoverPoolsCard.svelte';
  import LoadPoolCard from './lpTrade/LoadPoolCard.svelte';
  import PoolDetailCard from './lpTrade/PoolDetailCard.svelte';
  import PositionPanel from './lpTrade/PositionPanel.svelte';
  import RemoveLiquidityCard from './lpTrade/RemoveLiquidityCard.svelte';
  import SetupPoolCard from './lpTrade/SetupPoolCard.svelte';
  import SwapQuoteCard from './lpTrade/SwapQuoteCard.svelte';
  import {
    assetLabelFromMeta,
    assetMetaFromCache,
    externalPoolCanMutateLiquidity as canMutateExternalPoolLiquidity,
    fmt,
    formatAssetWithMeta,
    formatBps,
    formatLp,
    formatSharePpm,
    normalizeAssetId,
    poolStateDiffers,
    rawAmountFromDecimal as parseRawAmountFromDecimal,
    recommendedFeeForProfile,
    scaledRateToBps,
    sourceBadgeClass as classForSourceStatus,
    sourceCapability,
    sourceStatus as findSourceStatus,
  } from './lpTrade/viewModel.js';

  let status = null;
  let externalStatus = null;
  let pools = [];
  let pool = null;
  let quote = null;
  let error = '';
  let discoveryNote = '';
  let loadingStatus = false;
  let loadingPools = false;
  let loadingPool = false;
  let loadingQuote = false;
  let loadingCreatePrepare = false;
  let loadingCreateSubmit = false;
  let loadingSetupPrepare = false;
  let loadingSetupSubmit = false;
  let loadingBootstrapPrepare = false;
  let loadingBootstrapSubmit = false;
  let loadingAddPrepare = false;
  let loadingAddSubmit = false;
  let loadingRemovePrepare = false;
  let loadingRemoveSubmit = false;
  let loadingSwapPrepare = false;
  let loadingSwapSubmit = false;
  let loadingLpOptIn = false;
  let loadingPositions = false;
  let loadingCreateLookup = false;

  let assetA = '0';
  let assetB = '';
  let appId = '';
  let assetIn = '0';
  let amountIn = '';
  let slippageBps = 50;
  let createAssetA = '0';
  let createAssetB = '';
  let createFeeBps = 30;
  let createPairProfile = 'standard';
  let createAcknowledgeNoOwner = false;
  let createExistingPools = [];
  let createExistingPool = null;
  let createLookupNote = '';
  let createPin = '';
  let createPreview = null;
  let createResult = null;
  let setupAppId = '';
  let setupFundingMicroalgo = 500000;
  let setupPin = '';
  let setupPreview = null;
  let setupResult = null;
  let bootstrapAmount0 = '';
  let bootstrapAmount1 = '';
  let bootstrapPin = '';
  let bootstrapPreview = null;
  let bootstrapResult = null;
  let addDesired0 = '';
  let addDesired1 = '';
  let addPin = '';
  let addPreview = null;
  let addResult = null;
  let removeBurnLp = '';
  let removePin = '';
  let removePreview = null;
  let removeResult = null;
  let swapPin = '';
  let swapPreview = null;
  let swapResult = null;
  let expireRounds = 1000;
  let lpOptInPin = '';
  let lpOptInResult = null;
  let positions = [];
  let positionsNote = '';
  let assetMetaCache = new Map();
  let createLookupTimer = null;
  let lastPositionsAddress = '';
  let lastLpResetSeq = -1;

  $: currentWallet = $activeWallet;
  $: pairState = $tradeState;
  $: creator = currentWallet?.first_address || '';
  $: selectedPool = pool?.pool || null;
  $: selectedPoolIsNative = selectedPool?.source === 'native';
  $: selectedPoolCanSwap = selectedPoolIsNative || Boolean(selectedPool?.swap_supported);
  $: selectedPoolCanAddLiquidity = selectedPoolIsNative || externalPoolCanMutateLiquidity(selectedPool);
  $: selectedPoolCanRemoveLiquidity = selectedPoolIsNative || externalPoolCanMutateLiquidity(selectedPool);
  $: quoteIsStale = Boolean(quote?.pool && selectedPool && quote.pool.source_round !== selectedPool.source_round);
  $: if (pairState.resetSeq !== lastLpResetSeq) {
    lastLpResetSeq = pairState.resetSeq;
    clearLpTransientState();
  }
  $: if (creator && creator !== lastPositionsAddress) {
    lastPositionsAddress = creator;
    loadPositions();
  }

  function setError(err) {
    error = err?.message || String(err || '');
  }

  function selectedAssetFromInput(value) {
    const id = normalizeAssetId(value);
    return id == null ? null : assetMeta(id);
  }

  function onDiscoverAssetASelect(asset) {
    const selected = asset || { id: 0 };
    assetA = String(selected.id);
    setTradeBaseAsset(selected);
    ensureAssetMeta(selected.id);
  }

  function onDiscoverAssetBSelect(asset) {
    assetB = asset ? String(asset.id) : '';
    setTradeQuoteAsset(asset);
    if (asset) ensureAssetMeta(asset.id);
  }

  function onCreateAssetASelect(asset) {
    const selected = asset || { id: 0 };
    createAssetA = String(selected.id);
    setTradeBaseAsset(selected);
    ensureAssetMeta(selected.id);
    scheduleCreateLookup();
  }

  function onCreateAssetBSelect(asset) {
    createAssetB = asset ? String(asset.id) : '';
    setTradeQuoteAsset(asset);
    if (asset) ensureAssetMeta(asset.id);
    scheduleCreateLookup();
  }

  function switchCreatePair() {
    const currentA = createAssetA;
    createAssetA = createAssetB || '0';
    createAssetB = currentA;
    swapTradePair();
    scheduleCreateLookup();
  }

  function switchDiscoverPair() {
    const currentA = assetA;
    assetA = assetB || '0';
    assetB = currentA;
    swapTradePair();
  }

  function clearLpTransientState() {
    pools = [];
    pool = null;
    quote = null;
    createExistingPools = [];
    createExistingPool = null;
    createLookupNote = '';
    discoveryNote = '';
    swapPreview = null;
    swapResult = null;
    setQuoteStatus({ state: 'idle' });
  }

  function ensureSessionForSigning() {
    if (!getToken()) {
      throw new Error($t('common.sessionExpired'));
    }
  }

  function assetMeta(assetId) {
    return assetMetaFromCache(assetId, assetMetaCache);
  }

  function assetLabel(assetId) {
    return assetLabelFromMeta(assetMeta(assetId));
  }

  function formatAsset(raw, assetId) {
    return formatAssetWithMeta(raw, assetMeta(assetId));
  }

  function sourceStatus(source) {
    return findSourceStatus(externalStatus, source);
  }

  function sourceStatusLabel(source) {
    const capability = sourceCapability(externalStatus, source);
    if (capability === 'ready') return $t('dex.sourceReady');
    if (capability === 'quoteOnly') return $t('lpTrade.quoteOnly');
    return $t('dex.sourceNotConnected');
  }

  function sourceBadgeClass(source) {
    return classForSourceStatus(externalStatus, source);
  }

  function externalPoolCanMutateLiquidity(poolInfo) {
    return canMutateExternalPoolLiquidity(poolInfo, externalStatus);
  }

  async function ensureAssetMeta(assetId) {
    const id = Number(assetId);
    if (!Number.isSafeInteger(id) || id < 0 || id === 0 || assetMetaCache.has(id)) return;
    try {
      const meta = await api.getAsset(id);
      assetMetaCache = new Map(assetMetaCache).set(id, {
        id,
        name: meta.name || `#${id}`,
        unit: meta.unit || `#${id}`,
        decimals: meta.decimals ?? 6,
      });
    } catch (_) {
      assetMetaCache = new Map(assetMetaCache).set(id, { id, name: `#${id}`, unit: `#${id}`, decimals: 6 });
    }
  }

  async function ensurePoolAssetMeta(poolInfo) {
    if (!poolInfo) return;
    const extra = [];
    if (poolInfo.folks) {
      extra.push(ensureAssetMeta(poolInfo.folks.underlying_0));
      extra.push(ensureAssetMeta(poolInfo.folks.underlying_1));
    }
    await Promise.all([
      ensureAssetMeta(poolInfo.asset_0),
      ensureAssetMeta(poolInfo.asset_1),
      poolInfo.lp_asset_id ? ensureAssetMeta(poolInfo.lp_asset_id) : Promise.resolve(),
      ...extra,
    ]);
  }

  function rawAmountFromDecimal(value, assetId) {
    return parseRawAmountFromDecimal(value, assetId, assetMeta);
  }

  function currentSwapAmountRaw() {
    return rawAmountFromDecimal(amountIn, Number(assetIn));
  }

  function currentSwapAmountRawLabel() {
    try {
      return String(currentSwapAmountRaw());
    } catch (_) {
      return '—';
    }
  }

  async function requireFreshSwapQuote(selected) {
    if (!quote?.pool || !quote?.quote) {
      throw new Error($t('lpTrade.quoteRequired'));
    }
    const latest = selected.source === 'native'
      ? await api.getLpPool(selected.app_id)
      : await loadLatestExternalPool(selected);
    await ensurePoolAssetMeta(latest.pool);
    if (poolStateDiffers(quote.pool, latest.pool)) {
      pool = { pool: latest.pool };
      quote = null;
      throw new Error($t('lpTrade.quoteStale'));
    }
  }

  async function loadLatestExternalPool(selected) {
    const resp = await api.getExternalLpPools({
      assetA: selected.asset_0,
      assetB: selected.asset_1,
      source: selected.source,
    });
    const found = (resp.pools || []).find((item) => item.pool_id === selected.pool_id);
    if (!found) throw new Error($t('lpTrade.externalPoolMissing'));
    return { pool: found };
  }

  function applyCreatePairProfile() {
    createFeeBps = recommendedFeeForProfile(createPairProfile);
    scheduleCreateLookup();
  }

  function resetCreateIntent() {
    createPreview = null;
    createResult = null;
    createPin = '';
  }

  async function refreshCreateLookup() {
    const a = normalizeAssetId(createAssetA);
    const b = normalizeAssetId(createAssetB);
    createExistingPool = null;
    createExistingPools = [];
    createLookupNote = '';
    if (a == null || b == null || a === b) return;
    loadingCreateLookup = true;
    try {
      const resp = await api.getLpPools({ assetA: a, assetB: b });
      createExistingPools = resp.pools || [];
      createExistingPool = createExistingPools.find((item) => Number(item.fee_bps) === Number(createFeeBps)) || null;
      createLookupNote = resp.discovery_note || '';
      await Promise.all(createExistingPools.map((item) => ensurePoolAssetMeta(item)));
    } catch (err) {
      createLookupNote = err?.message || String(err || '');
    } finally {
      loadingCreateLookup = false;
    }
  }

  function scheduleCreateLookup() {
    resetCreateIntent();
    if (createLookupTimer) clearTimeout(createLookupTimer);
    createLookupTimer = setTimeout(refreshCreateLookup, 350);
  }

  async function loadPositions() {
    if (!creator) {
      positions = [];
      positionsNote = '';
      return;
    }
    loadingPositions = true;
    positionsNote = '';
    try {
      const native = await api.getLpPositions({ address: creator });
      const combined = [...(native.positions || [])];
      const notes = [native.discovery_note].filter(Boolean);
      const a = normalizeAssetId(assetA);
      const b = normalizeAssetId(assetB);
      if (a != null && b != null && a !== b) {
        try {
          const external = await api.getExternalLpPositions({ address: creator, assetA: a, assetB: b });
          combined.push(...(external.positions || []));
          if (external.discovery_note) notes.push(external.discovery_note);
        } catch (err) {
          notes.push(err?.message || String(err || ''));
        }
      }
      positions = combined;
      positionsNote = notes.join(' · ');
      await Promise.all(positions.map((item) => ensurePoolAssetMeta(item.pool)));
    } catch (err) {
      positions = [];
      positionsNote = err?.message || String(err || '');
    } finally {
      loadingPositions = false;
    }
  }

  async function choosePool(poolInfo) {
    pool = { pool: poolInfo };
    appId = String(poolInfo.app_id);
    setupAppId = String(poolInfo.app_id);
    assetIn = String(poolInfo.asset_0);
    quote = null;
    swapPreview = null;
    await ensurePoolAssetMeta(poolInfo);
  }

  async function loadStatus() {
    loadingStatus = true;
    error = '';
    try {
      const [lpStatus, extStatus] = await Promise.all([
        api.getLpStatus(),
        api.getExternalLiquidityStatus(),
      ]);
      status = lpStatus;
      externalStatus = extStatus;
    } catch (err) {
      setError(err);
    } finally {
      loadingStatus = false;
    }
  }

  async function discoverPools() {
    loadingPools = true;
    error = '';
    discoveryNote = '';
    pools = [];
    try {
      const [nativeResult, externalResult] = await Promise.allSettled([
        api.getLpPools({ assetA, assetB }),
        api.getExternalLpPools({ assetA, assetB }),
      ]);
      const nativeResp = nativeResult.status === 'fulfilled' ? nativeResult.value : { pools: [], discovery_note: nativeResult.reason?.message || String(nativeResult.reason || '') };
      const externalResp = externalResult.status === 'fulfilled' ? externalResult.value : { pools: [], discovery_note: externalResult.reason?.message || String(externalResult.reason || '') };
      pools = [...(nativeResp.pools || []), ...(externalResp.pools || [])];
      discoveryNote = [nativeResp.discovery_note, externalResp.discovery_note].filter(Boolean).join(' · ');
      await Promise.all(pools.map((item) => ensurePoolAssetMeta(item)));
      if (!pool && pools.length > 0) {
        await choosePool(pools[0]);
      }
    } catch (err) {
      setError(err);
    } finally {
      loadingPools = false;
    }
  }

  async function loadPool(id = appId) {
    if (!String(id || '').trim()) {
      error = $t('lpTrade.appIdRequired');
      return;
    }
    loadingPool = true;
    error = '';
    quote = null;
    try {
      const resp = await api.getLpPool(String(id).trim());
      await choosePool(resp.pool);
    } catch (err) {
      setError(err);
    } finally {
      loadingPool = false;
    }
  }

  async function loadQuote() {
    if (!pool?.pool) {
      error = $t('lpTrade.poolRequired');
      return;
    }
    if (!String(amountIn || '').trim()) {
      error = $t('lpTrade.amountRequired');
      return;
    }
    loadingQuote = true;
    error = '';
    quote = null;
    const selected = requirePool();
    const quoteSource = selected.source === 'native' ? 'OpenNodia Pool' : selected.source;
    setQuoteStatus({
      state: 'loading',
      source: quoteSource,
      message: $t('lpTrade.loadingQuote'),
    });
    try {
      if (selected.source === 'native') {
        quote = await api.quoteLpSwap({
          appId,
          assetIn,
          amountIn: currentSwapAmountRaw(),
          slippageBps,
        });
      } else {
        quote = await api.quoteExternalLiquidity({
          source: selected.source,
          poolId: selected.pool_id,
          assetIn,
          amountIn: currentSwapAmountRaw(),
          slippageBps,
        });
      }
      pool = { pool: quote.pool };
      await ensurePoolAssetMeta(quote.pool);
      setQuoteStatus({
        state: 'ready',
        source: quoteSource,
        message: $t('lpTrade.quoteReady'),
      });
    } catch (err) {
      setError(err);
      setQuoteStatus({
        state: 'error',
        source: quoteSource,
        message: err?.message || String(err || ''),
      });
    } finally {
      loadingQuote = false;
    }
  }

  function requireWallet() {
    if (!currentWallet || !creator) throw new Error($t('lpTrade.noWalletSelected'));
  }

  function requirePool() {
    if (!pool?.pool) throw new Error($t('lpTrade.poolRequired'));
    return pool.pool;
  }

  async function prepareCreatePool() {
    loadingCreatePrepare = true;
    error = '';
    createPreview = null;
    createResult = null;
    try {
      requireWallet();
      await refreshCreateLookup();
      if (createExistingPool) {
        appId = String(createExistingPool.app_id);
        await choosePool(createExistingPool);
        throw new Error($t('lpTrade.existingPoolFound', { appId: createExistingPool.app_id }));
      }
      if (!createAcknowledgeNoOwner) {
        throw new Error($t('lpTrade.creatorNoOwnerRequired'));
      }
      createPreview = await api.prepareLpPoolCreate({
        walletId: currentWallet.id,
        creator,
        assetA: createAssetA,
        assetB: createAssetB,
        feeBps: createFeeBps,
      });
    } catch (err) {
      setError(err);
    } finally {
      loadingCreatePrepare = false;
    }
  }

  async function submitCreatePool() {
    if (!createPin) {
      error = $t('lpTrade.pinRequired');
      return;
    }
    try {
      ensureSessionForSigning();
    } catch (err) {
      setError(err);
      return;
    }
    loadingCreateSubmit = true;
    error = '';
    try {
      requireWallet();
      createResult = await api.createLpPool({
        walletId: currentWallet.id,
        pin: createPin,
        intentId: createPreview?.intent_id,
        creator,
        assetA: createAssetA,
        assetB: createAssetB,
        feeBps: createFeeBps,
      });
      createPin = '';
      appId = String(createResult.app_id);
      setupAppId = String(createResult.app_id);
      await loadPool(createResult.app_id);
      await loadPositions();
    } catch (err) {
      setError(err);
    } finally {
      loadingCreateSubmit = false;
    }
  }

  async function prepareSetupPool() {
    loadingSetupPrepare = true;
    error = '';
    setupPreview = null;
    setupResult = null;
    try {
      requireWallet();
      setupPreview = await api.prepareLpPoolSetup({
        walletId: currentWallet.id,
        creator,
        appId: setupAppId,
        fundingMicroalgo: setupFundingMicroalgo,
      });
    } catch (err) {
      setError(err);
    } finally {
      loadingSetupPrepare = false;
    }
  }

  async function submitSetupPool() {
    if (!setupPin) {
      error = $t('lpTrade.pinRequired');
      return;
    }
    try {
      ensureSessionForSigning();
    } catch (err) {
      setError(err);
      return;
    }
    loadingSetupSubmit = true;
    error = '';
    try {
      requireWallet();
      setupResult = await api.setupLpPool({
        walletId: currentWallet.id,
        pin: setupPin,
        intentId: setupPreview?.intent_id,
        creator,
        appId: setupAppId,
        fundingMicroalgo: setupFundingMicroalgo,
      });
      setupPin = '';
      pool = { pool: setupResult.pool };
      appId = String(setupResult.pool.app_id);
      assetIn = String(setupResult.pool.asset_0);
      bootstrapPreview = null;
      addPreview = null;
      removePreview = null;
      swapPreview = null;
      await ensurePoolAssetMeta(setupResult.pool);
      await loadPositions();
    } catch (err) {
      setError(err);
    } finally {
      loadingSetupSubmit = false;
    }
  }

  async function submitLpAssetOptIn() {
    if (!lpOptInPin) {
      error = $t('lpTrade.pinRequired');
      return;
    }
    try {
      ensureSessionForSigning();
    } catch (err) {
      setError(err);
      return;
    }
    loadingLpOptIn = true;
    error = '';
    lpOptInResult = null;
    try {
      requireWallet();
      const selected = requirePool();
      if (!selected.lp_asset_id) throw new Error($t('lpTrade.lpAssetRequired'));
      lpOptInResult = await api.optInAsset({
        walletId: currentWallet.id,
        pin: lpOptInPin,
        address: creator,
        assetId: selected.lp_asset_id,
      });
      lpOptInPin = '';
      await loadPositions();
    } catch (err) {
      setError(err);
    } finally {
      loadingLpOptIn = false;
    }
  }

  async function prepareBootstrapPool() {
    loadingBootstrapPrepare = true;
    error = '';
    bootstrapPreview = null;
    bootstrapResult = null;
    try {
      requireWallet();
      const selected = requirePool();
      bootstrapPreview = await api.prepareLpPoolBootstrap({
        walletId: currentWallet.id,
        provider: creator,
        appId: selected.app_id,
        amount0: bootstrapAmount0,
        amount1: bootstrapAmount1,
        slippageBps,
        expireRounds,
      });
    } catch (err) {
      setError(err);
    } finally {
      loadingBootstrapPrepare = false;
    }
  }

  async function submitBootstrapPool() {
    if (!bootstrapPin) {
      error = $t('lpTrade.pinRequired');
      return;
    }
    try {
      ensureSessionForSigning();
    } catch (err) {
      setError(err);
      return;
    }
    loadingBootstrapSubmit = true;
    error = '';
    try {
      requireWallet();
      const selected = requirePool();
      bootstrapResult = await api.bootstrapLpPool({
        walletId: currentWallet.id,
        pin: bootstrapPin,
        intentId: bootstrapPreview?.intent_id,
        provider: creator,
        appId: selected.app_id,
        amount0: bootstrapAmount0,
        amount1: bootstrapAmount1,
        slippageBps,
        expireRounds,
      });
      bootstrapPin = '';
      pool = { pool: bootstrapResult.pool };
      assetIn = String(bootstrapResult.pool.asset_0);
      bootstrapPreview = null;
      await ensurePoolAssetMeta(bootstrapResult.pool);
      await loadPositions();
    } catch (err) {
      setError(err);
    } finally {
      loadingBootstrapSubmit = false;
    }
  }

  async function prepareAddLiquidity() {
    loadingAddPrepare = true;
    error = '';
    addPreview = null;
    addResult = null;
    try {
      requireWallet();
      const selected = requirePool();
      if (!selectedPoolCanAddLiquidity) {
        throw new Error($t('lpTrade.externalLiquidityReadOnly'));
      }
      if (selected.source === 'native') {
        addPreview = await api.prepareLpPoolAdd({
          walletId: currentWallet.id,
          provider: creator,
          appId: selected.app_id,
          desired0: addDesired0,
          desired1: addDesired1,
          slippageBps,
          expireRounds,
        });
      } else {
        addPreview = await api.prepareExternalAddLiquidity({
          walletId: currentWallet.id,
          source: selected.source,
          poolId: selected.pool_id,
          provider: creator,
          amount0: addDesired0,
          amount1: addDesired1,
          slippageBps,
          expireRounds,
        });
      }
    } catch (err) {
      setError(err);
    } finally {
      loadingAddPrepare = false;
    }
  }

  async function submitAddLiquidity() {
    if (!addPin) {
      error = $t('lpTrade.pinRequired');
      return;
    }
    try {
      ensureSessionForSigning();
    } catch (err) {
      setError(err);
      return;
    }
    loadingAddSubmit = true;
    error = '';
    try {
      requireWallet();
      const selected = requirePool();
      if (selected.source === 'native') {
        addResult = await api.addLpPoolLiquidity({
          walletId: currentWallet.id,
          pin: addPin,
          intentId: addPreview?.intent_id,
          provider: creator,
          appId: selected.app_id,
          desired0: addDesired0,
          desired1: addDesired1,
          slippageBps,
          expireRounds,
        });
      } else {
        addResult = await api.submitExternalAddLiquidity({
          walletId: currentWallet.id,
          pin: addPin,
          intentId: addPreview?.intent_id,
          source: selected.source,
          poolId: selected.pool_id,
          provider: creator,
          amount0: addDesired0,
          amount1: addDesired1,
          slippageBps,
          expireRounds,
        });
      }
      addPin = '';
      pool = { pool: addResult.pool };
      addPreview = null;
      await ensurePoolAssetMeta(addResult.pool);
      await loadPositions();
    } catch (err) {
      setError(err);
    } finally {
      loadingAddSubmit = false;
    }
  }

  async function prepareRemoveLiquidity() {
    loadingRemovePrepare = true;
    error = '';
    removePreview = null;
    removeResult = null;
    try {
      requireWallet();
      const selected = requirePool();
      if (!selectedPoolCanRemoveLiquidity) {
        throw new Error($t('lpTrade.externalLiquidityReadOnly'));
      }
      if (selected.source === 'native') {
        removePreview = await api.prepareLpPoolRemove({
          walletId: currentWallet.id,
          provider: creator,
          appId: selected.app_id,
          burnLp: removeBurnLp,
          slippageBps,
          expireRounds,
        });
      } else {
        removePreview = await api.prepareExternalRemoveLiquidity({
          walletId: currentWallet.id,
          source: selected.source,
          poolId: selected.pool_id,
          provider: creator,
          burnLp: removeBurnLp,
          slippageBps,
          expireRounds,
        });
      }
    } catch (err) {
      setError(err);
    } finally {
      loadingRemovePrepare = false;
    }
  }

  async function submitRemoveLiquidity() {
    if (!removePin) {
      error = $t('lpTrade.pinRequired');
      return;
    }
    try {
      ensureSessionForSigning();
    } catch (err) {
      setError(err);
      return;
    }
    loadingRemoveSubmit = true;
    error = '';
    try {
      requireWallet();
      const selected = requirePool();
      if (selected.source === 'native') {
        removeResult = await api.removeLpPoolLiquidity({
          walletId: currentWallet.id,
          pin: removePin,
          intentId: removePreview?.intent_id,
          provider: creator,
          appId: selected.app_id,
          burnLp: removeBurnLp,
          slippageBps,
          expireRounds,
        });
      } else {
        removeResult = await api.submitExternalRemoveLiquidity({
          walletId: currentWallet.id,
          pin: removePin,
          intentId: removePreview?.intent_id,
          source: selected.source,
          poolId: selected.pool_id,
          provider: creator,
          burnLp: removeBurnLp,
          slippageBps,
          expireRounds,
        });
      }
      removePin = '';
      pool = { pool: removeResult.pool };
      removePreview = null;
      await ensurePoolAssetMeta(removeResult.pool);
      await loadPositions();
    } catch (err) {
      setError(err);
    } finally {
      loadingRemoveSubmit = false;
    }
  }

  async function prepareSwapExecution() {
    loadingSwapPrepare = true;
    error = '';
    swapPreview = null;
    swapResult = null;
    try {
      requireWallet();
      const selected = requirePool();
      if (!selectedPoolCanSwap) {
        throw new Error($t('lpTrade.externalSwapReadOnly'));
      }
      await requireFreshSwapQuote(selected);
      if (selected.source === 'native') {
        swapPreview = await api.prepareLpSwap({
          walletId: currentWallet.id,
          trader: creator,
          appId: selected.app_id,
          assetIn,
          amountIn: currentSwapAmountRaw(),
          slippageBps,
          expireRounds,
        });
      } else {
        swapPreview = await api.prepareExternalSwap({
          walletId: currentWallet.id,
          source: selected.source,
          poolId: selected.pool_id,
          trader: creator,
          assetIn,
          amountIn: currentSwapAmountRaw(),
          slippageBps,
          expireRounds,
        });
      }
    } catch (err) {
      setError(err);
    } finally {
      loadingSwapPrepare = false;
    }
  }

  async function submitSwapExecution() {
    if (!swapPin) {
      error = $t('lpTrade.pinRequired');
      return;
    }
    try {
      ensureSessionForSigning();
    } catch (err) {
      setError(err);
      return;
    }
    loadingSwapSubmit = true;
    error = '';
    try {
      requireWallet();
      const selected = requirePool();
      if (selected.source === 'native') {
        swapResult = await api.swapLpExactIn({
          walletId: currentWallet.id,
          pin: swapPin,
          intentId: swapPreview?.intent_id,
          trader: creator,
          appId: selected.app_id,
          assetIn,
          amountIn: currentSwapAmountRaw(),
          slippageBps,
          expireRounds,
        });
      } else {
        swapResult = await api.submitExternalSwap({
          walletId: currentWallet.id,
          pin: swapPin,
          intentId: swapPreview?.intent_id,
          source: selected.source,
          poolId: selected.pool_id,
          trader: creator,
          assetIn,
          amountIn: currentSwapAmountRaw(),
          slippageBps,
          expireRounds,
        });
      }
      swapPin = '';
      pool = { pool: swapResult.pool };
      quote = { pool: swapResult.pool, quote: swapResult.quote };
      swapPreview = null;
      await ensurePoolAssetMeta(swapResult.pool);
      await loadPositions();
    } catch (err) {
      setError(err);
    } finally {
      loadingSwapSubmit = false;
    }
  }

  onMount(() => {
    loadStatus();
    loadPositions();
  });
</script>

<div class="space-y-6">
  <div>
    <h2 class="text-2xl font-bold text-gray-100">{$t('lpTrade.title')}</h2>
    <p class="mt-1 text-sm text-gray-500">{$t('lpTrade.subtitle')}</p>
  </div>

  {#if error}
    <div class="rounded-xl border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-300">
      {error}
    </div>
  {/if}

  <section class="card">
    <div class="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
      <div>
        <h3 class="text-lg font-semibold text-gray-100">{$t('lpTrade.nativeStatus')}</h3>
      </div>
      <button class="btn-secondary shrink-0" type="button" on:click={loadStatus} disabled={loadingStatus}>
        {loadingStatus ? $t('common.loading') : $t('common.refresh')}
      </button>
    </div>
  </section>

  <PositionPanel
    {positions}
    {positionsNote}
    {loadingPositions}
    {creator}
    {assetLabel}
    {formatAsset}
    {formatLp}
    {formatSharePpm}
    onRefresh={loadPositions}
    onSelectPool={choosePool}
  />

  <section class="grid gap-6 lg:grid-cols-2">
    <CreatePoolCard
      {createAssetA}
      {createAssetB}
      bind:createPairProfile
      bind:createFeeBps
      bind:createAcknowledgeNoOwner
      {loadingCreateLookup}
      {createExistingPool}
      {createExistingPools}
      {createLookupNote}
      {loadingCreatePrepare}
      {currentWallet}
      {createPreview}
      {setupFundingMicroalgo}
      bind:createPin
      {loadingCreateSubmit}
      {createResult}
      {selectedAssetFromInput}
      {onCreateAssetASelect}
      {onCreateAssetBSelect}
      {switchCreatePair}
      {applyCreatePairProfile}
      {scheduleCreateLookup}
      {choosePool}
      {prepareCreatePool}
      {submitCreatePool}
    />

    <SetupPoolCard
      bind:setupAppId
      bind:setupFundingMicroalgo
      {loadingSetupPrepare}
      {currentWallet}
      {setupPreview}
      bind:setupPin
      {loadingSetupSubmit}
      {setupResult}
      {prepareSetupPool}
      {submitSetupPool}
    />
  </section>

  <section class="grid gap-6 lg:grid-cols-2">
    <DiscoverPoolsCard
      {assetA}
      {assetB}
      {loadingPools}
      {discoveryNote}
      {pools}
      {selectedAssetFromInput}
      {sourceBadgeClass}
      {sourceStatusLabel}
      {onDiscoverAssetASelect}
      {onDiscoverAssetBSelect}
      {switchDiscoverPair}
      {discoverPools}
      {loadPool}
      {choosePool}
      {assetLabel}
      {formatAsset}
    />

    <LoadPoolCard
      bind:appId
      {loadingPool}
      {loadPool}
    />
  </section>

  {#if pool?.pool}
    <PoolDetailCard
      {pool}
      bind:lpOptInPin
      {loadingLpOptIn}
      {currentWallet}
      {lpOptInResult}
      {submitLpAssetOptIn}
      {assetLabel}
      {formatAsset}
      {formatBps}
      {formatLp}
      {scaledRateToBps}
    />

    <section class="grid gap-6 lg:grid-cols-3">
      <BootstrapPoolCard
        {pool}
        bind:bootstrapAmount0
        bind:bootstrapAmount1
        {loadingBootstrapPrepare}
        {currentWallet}
        {selectedPoolIsNative}
        {bootstrapPreview}
        bind:bootstrapPin
        {loadingBootstrapSubmit}
        {bootstrapResult}
        {prepareBootstrapPool}
        {submitBootstrapPool}
        {formatAsset}
        {formatLp}
      />

      <AddLiquidityCard
        {pool}
        bind:addDesired0
        bind:addDesired1
        {loadingAddPrepare}
        {currentWallet}
        {selectedPoolCanAddLiquidity}
        {addPreview}
        bind:addPin
        {loadingAddSubmit}
        {addResult}
        {prepareAddLiquidity}
        {submitAddLiquidity}
        {formatAsset}
        {formatLp}
      />

      <RemoveLiquidityCard
        {pool}
        bind:removeBurnLp
        {loadingRemovePrepare}
        {currentWallet}
        {selectedPoolCanRemoveLiquidity}
        {removePreview}
        bind:removePin
        {loadingRemoveSubmit}
        {removeResult}
        {prepareRemoveLiquidity}
        {submitRemoveLiquidity}
        {formatAsset}
      />
    </section>

    <SwapQuoteCard
      {pool}
      bind:assetIn
      bind:amountIn
      bind:slippageBps
      bind:expireRounds
      {loadingQuote}
      {loadingSwapPrepare}
      {currentWallet}
      {selectedPoolCanSwap}
      {selectedPoolIsNative}
      {quote}
      {quoteIsStale}
      {pairState}
      {swapPreview}
      bind:swapPin
      {loadingSwapSubmit}
      {swapResult}
      {assetLabel}
      {formatAsset}
      {fmt}
      {currentSwapAmountRawLabel}
      {loadQuote}
      {prepareSwapExecution}
      {submitSwapExecution}
    />
  {/if}
</div>
