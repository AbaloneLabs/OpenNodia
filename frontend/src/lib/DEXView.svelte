<script>
  import { onMount, onDestroy } from 'svelte';
  import { api, getToken } from '../api.js';
  import { formatRawAmount } from '../amount.js';
  import { t } from '../i18n/index.js';
  import { pendingOrderLinkPayload } from '../nav.js';
  import { orderLinkUiState } from '../orderLinkState.js';
  import {
    setQuoteStatus,
    setTradeBaseAsset,
    setTradeQuoteAsset,
    swapTradePair,
    tradeState,
  } from '../tradeState.js';
  import { walletList, activeWallet } from '../walletStore.js';
  import ChartPanel from './dex/ChartPanel.svelte';
  import CommunityMarketsPanel from './dex/CommunityMarketsPanel.svelte';
  import MarketHeader from './dex/MarketHeader.svelte';
  import MobileTradingTabs from './dex/MobileTradingTabs.svelte';
  import MyOrdersPanel from './dex/MyOrdersPanel.svelte';
  import OrderBookPanel from './dex/OrderBookPanel.svelte';
  import OrderFormPanel from './dex/OrderFormPanel.svelte';
  import OrderLinkPanel from './dex/OrderLinkPanel.svelte';
  import PopularPairsNav from './dex/PopularPairsNav.svelte';
  import RecentTradesPanel from './dex/RecentTradesPanel.svelte';
  import CancelConfirmModal from './dex/modals/CancelConfirmModal.svelte';
  import CreateConfirmModal from './dex/modals/CreateConfirmModal.svelte';
  import FillConfirmModal from './dex/modals/FillConfirmModal.svelte';
  import {
    CHART_RANGES,
    EXPIRY_PRESETS,
    assetRawBalanceFromAccount,
    cancelBatchRecoverableAlgo as collectCancelBatchRecoverableAlgo,
    cancelBatchRecoverableAssets as collectCancelBatchRecoverableAssets,
    depthWidth,
    deriveOrderParams as buildOrderParams,
    dexStatusClass,
    dexStatusLabel,
    estimatedExpiryText,
    formatAssetIdLabel,
    formatDisplayAmount,
    formatMicroPrice,
    humanPrice as priceFromMicro,
    isFirstActiveSplitChild as isFirstActiveSplitOrder,
    pairKey,
    pairLabelFor,
    presetIdForRounds,
    relativeTimeLabel,
    roundsForSeconds,
    splitActiveOrders as collectSplitActiveOrders,
    splitChildCount as countSplitChildren,
    splitGroupOrders as collectSplitGroupOrders,
    splitProgress as calculateSplitProgress,
  } from './dex/viewModel.js';

  // ---- State ----
  let loading = false;
  let error = '';
  let info = '';

  // Orderbook
  let book = null; // { bids, asks, spread, last_price, pair, ... }
  let bookLoading = false;
  let routeCandidates = null;
  let routeLoading = false;
  let routeError = '';

  // Order form
  let side = 'buy'; // 'buy' | 'sell'
  let orderPrice = ''; // price in quote-per-base (micro-ratio displayed as decimal)
  let orderAmount = ''; // amount of base asset
  let expireRounds = 10000;
  let splitCount = 1;
  let accountBalance = null; // fetched for Max button

  // Mobile view tab: which panel is shown on narrow screens (lg breakpoint).
  // 'chart' | 'orderbook' | 'order' — defaults to 'order' so the action panel
  // is immediately reachable without scrolling past the book.
  let mobileTab = 'order';

  // Immediate-or-Cancel (IOC) mode: when enabled, the order is matched against
  // the live orderbook first; any unfilled remainder is discarded (no standing
  // order is created). Uses the /api/dex/prepare/route + submit/route endpoints.
  let iocMode = false;
  let fillThenPlace = false;

  let expiryPreset = '1d';

  function selectExpiryPreset(id) {
    expiryPreset = id;
    const preset = EXPIRY_PRESETS.find((p) => p.id === id);
    expireRounds = preset ? roundsForSeconds(preset.seconds) : expireRounds;
  }

  // Keep expireRounds and the selected preset in sync for manual edits.
  function onExpireRoundsInput(e) {
    expireRounds = Number(e.target.value) || 0;
    expiryPreset = presetIdForRounds(expireRounds);
  }

  // My orders
  let myOrders = [];
  let myOrdersLoading = false;
  let ordersTab = 'active'; // 'active' | 'history'
  let orderLinkDetail = null;
  let orderLinkLoading = false;
  let orderLinkError = '';
  let lastOrderLinkPayload = '';

  // Recent trades
  let trades = [];
  let tradesLoading = false;
  let chartRange = 'all';
  // Popular pairs sidebar (Phase 3)
  let pairs = []; // [{ asset_a, asset_b, last_price, volume_24h, trade_count_24h, active_orders }]
  let pairsLoading = false;
  let selectedPairKey = ''; // canonical "asset_a:asset_b" of the active pair

  // Community markets: operator-signed official pair directories.
  let communityMarkets = [];
  let communityMarketsLoading = false;
  let communityMarketError = '';
  let lastCommunityMarketKey = '';

  // Prepare/submit flow state
  let pendingPrepare = null; // { intent_id, kind, owner_txs, logicsig_txs }
  let createPin = '';
  let submitting = false;

  // Fill flow
  let fillTarget = null; // order being filled
  let fillPrepare = null; // { filler_tx, verification }
  let fillPin = '';

  // Cancel flow
  let cancelTarget = null;
  let cancelPrepare = null;
  let cancelBatch = [];
  let cancelPrepares = [];
  let cancelPin = '';

  let pollTimer = null;
  let lastTradeResetSeq = -1;
  let lastLoadedBookPairKey = '';


  // ---- Derived ----
  $: currentWallet = $activeWallet;
  $: signer = currentWallet?.first_address || '';
  $: pairState = $tradeState;
  $: baseAssetId = Number(pairState.baseAsset?.id ?? 0);
  $: quoteAssetId = pairState.quoteAsset ? String(pairState.quoteAsset.id) : '';
  $: baseAssetMeta = pairState.baseAsset;
  $: quoteAssetMeta = pairState.quoteAsset;
  $: commonQuoteStatus = pairState.quoteStatus;
  $: baseLabel = baseAssetId == 0 ? 'ALGO' : (baseAssetMeta?.unit || `#${baseAssetId}`);
  $: quoteLabel = quoteAssetId ? (quoteAssetMeta?.unit || `#${quoteAssetId}`) : '';
  $: pairLabel = quoteAssetId ? `${quoteLabel} / ${baseLabel}` : '';
  // Total = price × amount (both in human-readable decimal, result in quote units)
  $: orderTotal = orderPrice && orderAmount ? (Number(orderPrice) * Number(orderAmount)).toFixed(6) : '';
  $: estimatedExpiry = estimatedExpiryText(expireRounds, $t);
  $: effectiveSplitCount = Math.max(1, Math.min(20, Number(splitCount) || 1));
  $: if (iocMode && Number(splitCount) !== 1) splitCount = 1;
  $: if (iocMode && fillThenPlace) fillThenPlace = false;
  $: perOrderAmount =
    Number(orderAmount) > 0
      ? (Number(orderAmount) / effectiveSplitCount).toLocaleString(undefined, {
          maximumFractionDigits: 8,
        })
      : '0';
  $: estimatedEscrowCost = (effectiveSplitCount * 0.2).toFixed(1);
  $: orderLinkUi = orderLinkUiState(orderLinkDetail, orderLinkError);
  $: if (pairState.resetSeq !== lastTradeResetSeq) {
    lastTradeResetSeq = pairState.resetSeq;
    clearPairDependentState();
  }
  $: if (getToken()) {
    const communityKey = quoteAssetId ? `${baseAssetId}:${quoteAssetId}` : 'all';
    if (communityKey !== lastCommunityMarketKey && !communityMarketsLoading) {
      loadCommunityMarkets();
    }
  }

  // ---- Lifecycle ----
  onMount(async () => {
    // Sync expireRounds with the default preset (1d).
    selectExpiryPreset(expiryPreset);
    await loadWallets();
    await loadPairs();
    await loadCommunityMarkets();
    pollTimer = setInterval(() => {
      if (quoteAssetId) refreshBook();
      // Refresh popular pairs every poll cycle too.
      loadPairs();
      loadCommunityMarkets();
    }, 15000);
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);

  });

  // ---- Data loaders ----
  async function loadWallets() {
    try {
      const wallets = await api.listWallets();
      walletList.set(wallets);
      if (!$activeWallet) {
        const active = await api.getActiveWallet();
        activeWallet.set(active.wallet);
      }
    } catch (e) {
      // ignore
    }
  }

  async function refreshBook() {
    if (!quoteAssetId) return;
    bookLoading = true;
    error = '';
    setQuoteStatus({
      state: 'loading',
      source: 'Orderbook',
      message: $t('dex.loading'),
    });
    try {
      const b = Number(quoteAssetId);
      book = await api.getOrderbook(baseAssetId, b, 20);
      setQuoteStatus({
        state: 'ready',
        source: 'Orderbook',
        message: $t('dex.orderbookQuoteReady'),
      });
      // Also load trades for this pair
      loadTrades();
    } catch (e) {
      error = e.message;
      book = null;
      setQuoteStatus({
        state: 'error',
        source: 'Orderbook',
        message: e.message,
      });
    } finally {
      bookLoading = false;
    }
  }

  async function refreshRouteCandidates() {
    routeError = '';
    routeCandidates = null;
    if (!quoteAssetId || !orderAmount || !orderPrice) {
      routeError = $t('dex.fillAllFields');
      return;
    }
    if (Number(orderPrice) <= 0 || Number(orderAmount) <= 0) {
      routeError = $t('dex.invalidPriceAmount');
      return;
    }
    try {
      routeLoading = true;
      const [sellAssetId, sellAmount, buyAssetId] = deriveOrderParams(side, orderAmount, orderPrice);
      routeCandidates = await api.getRouterQuote({
        assetIn: sellAssetId,
        assetOut: buyAssetId,
        amountIn: sellAmount,
        slippageBps: 50,
        depth: 20,
        source: 'best',
      });
      setQuoteStatus({
        state: routeCandidates.candidates?.length ? 'ready' : 'idle',
        source: 'Unified route',
        message: routeCandidates.candidates?.length
          ? $t('dex.routeCandidatesReady')
          : $t('dex.noRouteCandidates'),
      });
    } catch (e) {
      routeError = e.message;
      setQuoteStatus({
        state: 'error',
        source: 'Unified route',
        message: e.message,
      });
    } finally {
      routeLoading = false;
    }
  }

  async function loadMyOrders() {
    if (!signer) return;
    myOrdersLoading = true;
    try {
      const resp = await api.getMyOrders(signer, 'all');
      myOrders = resp.orders || [];
    } catch (e) {
      myOrders = [];
    } finally {
      myOrdersLoading = false;
    }
  }

  function splitGroupOrders(order) {
    return collectSplitGroupOrders(myOrders, order);
  }

  function splitActiveOrders(order) {
    return collectSplitActiveOrders(myOrders, order);
  }

  function splitChildCount(order) {
    return countSplitChildren(myOrders, order);
  }

  function splitProgress(order) {
    return calculateSplitProgress(myOrders, order);
  }

  function isFirstActiveSplitChild(order) {
    return isFirstActiveSplitOrder(myOrders, order);
  }

  function cancelBatchRecoverableAlgo() {
    return collectCancelBatchRecoverableAlgo(cancelPrepares);
  }

  function cancelBatchRecoverableAssets() {
    return collectCancelBatchRecoverableAssets(cancelPrepares);
  }

  async function loadOrderLink(payload) {
    if (!payload || payload === lastOrderLinkPayload) return;
    lastOrderLinkPayload = payload;
    orderLinkLoading = true;
    orderLinkError = '';
    orderLinkDetail = null;
    try {
      orderLinkDetail = await api.getOrderLinkDetail(payload);
      const decoded = orderLinkDetail.decoded;
      if (decoded) {
        const [sellMeta, buyMeta] = await Promise.all([
          resolveAssetMeta(decoded.sell_asset),
          resolveAssetMeta(decoded.buy_asset),
        ]);
        if (decoded.side === 'sell') {
          onBaseSelect(sellMeta);
          onQuoteSelect(buyMeta);
        } else {
          onBaseSelect(buyMeta);
          onQuoteSelect(sellMeta);
        }
      }
      info = orderLinkDetail.verification?.valid
        ? $t('dex.orderLinkLoaded')
        : $t('dex.orderLinkLoadedWithWarning');
    } catch (e) {
      orderLinkError = e.message;
    } finally {
      orderLinkLoading = false;
    }
  }

  function absoluteOrderUrl(url) {
    if (!url) return '';
    if (url.startsWith('http://') || url.startsWith('https://')) return url;
    return `${window.location.origin}${url}`;
  }

  async function copyText(text) {
    if (!text) return;
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }
    const el = document.createElement('textarea');
    el.value = text;
    el.setAttribute('readonly', '');
    el.style.position = 'absolute';
    el.style.left = '-9999px';
    document.body.appendChild(el);
    el.select();
    document.execCommand('copy');
    document.body.removeChild(el);
  }

  async function copyOrderLink(order) {
    try {
      const link = await api.getOrderLink(order.escrow_addr);
      await copyText(absoluteOrderUrl(link.url));
      info = $t('dex.orderLinkCopied');
      error = '';
    } catch (e) {
      error = e.message;
    }
  }

  async function copyOpenedOrderLink() {
    if (!orderLinkDetail?.url) return;
    try {
      await copyText(absoluteOrderUrl(orderLinkDetail.url));
      info = $t('dex.orderLinkCopied');
      error = '';
    } catch (e) {
      error = e.message;
    }
  }

  async function loadTrades() {
    if (!quoteAssetId) return;
    tradesLoading = true;
    try {
      const pair = `${baseAssetId}:${quoteAssetId}`;
      const resp = await api.getTrades({ pair, limit: 30 });
      trades = resp.trades || [];
    } catch (e) {
      trades = [];
    } finally {
      tradesLoading = false;
    }
  }

  async function loadBalance() {
    if (!signer) return;
    try {
      const acct = await api.getAccount(signer);
      accountBalance = acct;
    } catch (e) {
      accountBalance = null;
    }
  }

  // Popular pairs sidebar (Phase 3). Refreshed periodically.
  async function loadPairs() {
    pairsLoading = true;
    try {
      const resp = await api.getPairs(12);
      pairs = resp.pairs || [];
    } catch (e) {
      pairs = [];
    } finally {
      pairsLoading = false;
    }
  }

  async function loadCommunityMarkets() {
    const key = quoteAssetId ? `${baseAssetId}:${quoteAssetId}` : 'all';
    lastCommunityMarketKey = key;
    communityMarketsLoading = true;
    communityMarketError = '';
    try {
      const assetId = quoteAssetId ? (Number(baseAssetId) === 0 ? Number(quoteAssetId) : Number(baseAssetId)) : null;
      const resp = await api.listCommunityMarkets({ assetId, limit: 12 });
      communityMarkets = resp.markets || [];
    } catch (e) {
      communityMarkets = [];
      communityMarketError = e.message;
    } finally {
      communityMarketsLoading = false;
    }
  }

  function communityPairMatchesCurrent(pair) {
    if (!quoteAssetId) return false;
    const a = Math.min(Number(baseAssetId), Number(quoteAssetId));
    const b = Math.max(Number(baseAssetId), Number(quoteAssetId));
    return Number(pair.asset_a) === a && Number(pair.asset_b) === b;
  }

  function primaryCommunityPair(market) {
    if (!market?.pairs?.length) return null;
    return market.pairs.find(communityPairMatchesCurrent) || market.pairs[0];
  }

  async function selectCommunityPair(market, pair = primaryCommunityPair(market)) {
    if (!pair) return;
    await selectPair({ asset_a: pair.asset_a, asset_b: pair.asset_b });
  }

  function marketBadgeClass(market) {
    return market.official
      ? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-300'
      : 'border-yellow-500/40 bg-yellow-500/10 text-yellow-300';
  }

  function marketWarning(market) {
    return market?.warnings?.[0] || '';
  }

  // Clicking a popular pair loads it into the selectors.
  async function selectPair(p) {
    // p.asset_a / p.asset_b are the canonical pair. We map asset_a -> base,
    // asset_b -> quote so the orderbook query matches.
    onBaseSelect(assetMetaFromId(p.asset_a));
    onQuoteSelect(assetMetaFromId(p.asset_b));
    orderPrice = '';
    orderAmount = '';
    const selectedBase = Number(p.asset_a);
    const selectedQuote = Number(p.asset_b);
    const [baseMeta, quoteMeta] = await Promise.all([
      resolveAssetMeta(selectedBase),
      resolveAssetMeta(selectedQuote),
    ]);
    onBaseSelect(baseMeta);
    onQuoteSelect(quoteMeta);
  }

  // Build a minimal asset meta object from an id (ALGO special-case).
  function assetMetaFromId(id) {
    if (id === 0 || id === '0') {
      return { id: 0, name: 'Algo', unit: 'ALGO', decimals: 6 };
    }
    return { id: Number(id), name: `#${id}`, unit: `#${id}`, decimals: 6 };
  }

  async function resolveAssetMeta(id) {
    if (Number(id) === 0) return assetMetaFromId(0);
    try {
      const meta = await api.getAssetMetadata(Number(id));
      return {
        id: Number(meta.id),
        name: meta.name || `#${id}`,
        unit: meta.unit || `#${id}`,
        decimals: meta.decimals ?? 6,
      };
    } catch {
      return assetMetaFromId(id);
    }
  }

  // ---- Pair selector ----
  function onQuoteSelect(asset) {
    setTradeQuoteAsset(asset);
    clearPairDependentState();
  }

  function onBaseSelect(asset) {
    setTradeBaseAsset(asset);
    clearPairDependentState();
  }

  function swapPair() {
    if (!quoteAssetId) return;
    swapTradePair();
    orderPrice = '';
    orderAmount = '';
    clearPairDependentState();
  }

  function clearPairDependentState() {
    lastLoadedBookPairKey = '';
    book = null;
    trades = [];
    routeCandidates = null;
    routeError = '';
    pendingPrepare = null;
    fillPrepare = null;
    fillTarget = null;
    cancelPrepare = null;
    cancelTarget = null;
    cancelBatch = [];
    cancelPrepares = [];
    orderPrice = '';
    orderAmount = '';
    info = '';
    error = '';
    setQuoteStatus({ state: 'idle' });
  }

  // ---- Orderbook click-to-fill ----
  function clickPrice(level) {
    orderPrice = priceFromMicro(level.price, currentBaseDecimals(), currentQuoteDecimals()).toString();
  }

  function clickAmount(level) {
    orderAmount = formatRawAmount(level.amount, currentBaseDecimals());
  }

  function clickLevel(level) {
    clickPrice(level);
    clickAmount(level);
  }

  function handleLevelKeydown(event, level) {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    clickLevel(level);
  }

  // ---- Max button ----
  function setMaxAmount() {
    if (!accountBalance) {
      loadBalance();
      return;
    }
    // For buy: we spend quote asset; for sell: we spend base asset.
    if (side === 'buy') {
      // Buying base asset, paying with quote asset.
      // maxAmount (base units) = quoteBalance / price
      const price = Number(orderPrice);
      if (!price || price <= 0) return;
      const quoteDecimals = currentQuoteDecimals();
      const quoteBalanceRaw = assetRawBalance(Number(quoteAssetId || 0));
      const maxBaseRaw = Math.floor(quoteBalanceRaw / (price * 10 ** quoteDecimals));
      orderAmount = formatRawAmount(maxBaseRaw, currentBaseDecimals());
    } else {
      // Selling base asset.
      const baseBalanceRaw = assetRawBalance(Number(baseAssetId || 0));
      if (baseAssetId == 0) {
        // ALGO: subtract min-balance + fee
        const minBalance = accountBalance['min-balance'] || 0;
        const fee = 1000; // 0.001 ALGO
        const spendable = Math.max(0, baseBalanceRaw - minBalance - fee);
        orderAmount = formatRawAmount(spendable, currentBaseDecimals());
      } else {
        orderAmount = formatRawAmount(baseBalanceRaw, currentBaseDecimals());
      }
    }
  }

  // Get raw balance for a given asset id (0 = ALGO) from accountBalance.
  function assetRawBalance(assetId) {
    return assetRawBalanceFromAccount(accountBalance, assetId);
  }

  // ---- Helper: derive sell/buy params from pair-aware form ----
  // For a BUY order: we buy base asset, paying with quote asset.
  //   sell_asset = quote, sell_amount = amount × price, buy_asset = base, buy_amount = amount
  // For a SELL order: we sell base asset, receiving quote asset.
  //   sell_asset = base, sell_amount = amount, buy_asset = quote, buy_amount = amount × price
  function deriveOrderParams(currentSide, amount, price) {
    return buildOrderParams({
      side: currentSide,
      amount,
      price,
      baseAssetId,
      quoteAssetId,
      baseDecimals: currentBaseDecimals(),
      quoteDecimals: currentQuoteDecimals(),
    });
  }

  // ---- Create order (prepare/submit) ----
  async function prepareCreate() {
    if (!signer) {
      error = $t('dex.noWalletSelected');
      return;
    }
    if (!orderAmount || !orderPrice || !quoteAssetId) {
      error = $t('dex.fillAllFields');
      return;
    }
    if (Number(orderPrice) <= 0 || Number(orderAmount) <= 0) {
      error = $t('dex.invalidPriceAmount');
      return;
    }
    if (Number(baseAssetId) === Number(quoteAssetId)) {
      error = $t('dex.sameAssetPair');
      return;
    }
    loading = true;
    error = '';
    pendingPrepare = null;
    try {
      const [sellAssetId, sellAmount, buyAssetId, buyAmount] = deriveOrderParams(
        side,
        orderAmount,
        orderPrice,
      );
      if (iocMode) {
        const quote = await api.getRouterQuote({
          assetIn: sellAssetId,
          assetOut: buyAssetId,
          amountIn: sellAmount,
          slippageBps: 50,
          depth: 20,
          source: 'best',
        });
        routeCandidates = quote;
        const selected = quote.selected;
        if (!selected || selected.amount_out < buyAmount) {
          error = $t('dex.iocNoMatch');
          return;
        }
        const resp = await api.prepareRouter({
          walletId: currentWallet.id,
          trader: signer,
          quote: { ...quote, depth: 20, source: 'best' },
          routeHash: selected.route_hash,
          expireRounds: Number(expireRounds || 10000),
        });
        pendingPrepare = { ...resp, ioc: true, routed: true, router: true };
        info = $t('dex.reviewRoute');
        setQuoteStatus({
          state: 'ready',
          source: selected.source_label,
          message: $t('dex.instantPreviewReady'),
        });
      } else if (fillThenPlace) {
        const quote = await api.getRouterQuote({
          assetIn: sellAssetId,
          assetOut: buyAssetId,
          amountIn: sellAmount,
          slippageBps: 50,
          depth: 20,
          source: 'best',
        });
        routeCandidates = quote;
        const selected = quote.selected;
        if (selected && selected.minimum_out >= buyAmount && selected.remaining_input === 0) {
          const resp = await api.prepareRouter({
            walletId: currentWallet.id,
            trader: signer,
            quote: { ...quote, depth: 20, source: 'best' },
            routeHash: selected.route_hash,
            expireRounds: Number(expireRounds || 10000),
          });
          pendingPrepare = {
            ...resp,
            routed: true,
            router: true,
            placeRemaining: true,
            new_orders_needed: 0,
          };
          info = $t('dex.reviewFillThenPlace');
          setQuoteStatus({
            state: 'ready',
            source: selected.source_label,
            message: $t('dex.fillThenPlacePreviewReady'),
          });
          return;
        }
        const resp = await api.prepareRoute(
          currentWallet.id,
          signer,
          side,
          sellAssetId,
          sellAmount,
          buyAssetId,
          buyAmount,
          effectiveSplitCount,
          true,
          Number(expireRounds || 10000),
          true,
        );
        if (!resp.intent_id) {
          error = $t('dex.iocNoMatch');
          return;
        }
        pendingPrepare = { ...resp, routed: true, placeRemaining: true };
        info = $t('dex.reviewFillThenPlace');
        setQuoteStatus({
          state: 'ready',
          source: 'Orderbook',
          message: $t('dex.fillThenPlacePreviewReady'),
        });
      } else {
        const resp = await api.prepareCreate(
          currentWallet.id,
          signer,
          side,
          sellAssetId,
          sellAmount,
          buyAssetId,
          buyAmount,
          Number(expireRounds || 10000),
          effectiveSplitCount,
        );
        pendingPrepare = resp;
        info = $t('dex.reviewCreate');
        setQuoteStatus({
          state: 'ready',
          source: 'Orderbook',
          message: $t('dex.standingPreviewReady'),
        });
      }
    } catch (e) {
      error = e.message;
      setQuoteStatus({
        state: 'error',
        source: 'Orderbook',
        message: e.message,
      });
    } finally {
      loading = false;
    }
  }

  function closeModals() {
    pendingPrepare = null;
    fillPrepare = null;
    fillTarget = null;
    cancelPrepare = null;
    cancelTarget = null;
    cancelBatch = [];
    cancelPrepares = [];
    createPin = '';
    fillPin = '';
    cancelPin = '';
    error = '';
    info = '';
  }

  function focusTrap(node) {
    const previous = document.activeElement;
    const focusableSelector = [
      'a[href]',
      'button:not([disabled])',
      'input:not([disabled])',
      'select:not([disabled])',
      'textarea:not([disabled])',
      '[tabindex]:not([tabindex="-1"])',
    ].join(',');
    const focusFirst = () => {
      const first = node.querySelector(focusableSelector);
      (first || node).focus();
    };
    setTimeout(focusFirst, 0);

    function onKeydown(event) {
      if (event.key === 'Escape') {
        event.preventDefault();
        closeModals();
        return;
      }
      if (event.key !== 'Tab') return;
      const focusable = Array.from(node.querySelectorAll(focusableSelector)).filter(
        (el) => el.offsetParent !== null || el === document.activeElement,
      );
      if (!focusable.length) {
        event.preventDefault();
        node.focus();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }

    node.addEventListener('keydown', onKeydown);
    return {
      destroy() {
        node.removeEventListener('keydown', onKeydown);
        if (previous && typeof previous.focus === 'function') {
          previous.focus();
        }
      },
    };
  }

  async function submitCreate() {
    if (!pendingPrepare) return;
    if (!getToken()) {
      error = $t('common.sessionExpired');
      return;
    }
    if (!createPin) {
      error = $t('transfer.pinRequired');
      return;
    }
    submitting = true;
    error = '';
    try {
      if (pendingPrepare.routed) {
        const result = pendingPrepare.router
          ? await api.submitRouter({
              walletId: currentWallet.id,
              pin: createPin,
              intentId: pendingPrepare.intent_id,
              quoteId: pendingPrepare.quote_id,
              routeHash: pendingPrepare.route_hash,
            })
          : await api.submitRoute(currentWallet.id, createPin, pendingPrepare.intent_id);
        info =
          result.result?.fills?.some((fill) => fill.status === 'confirmed_unrecorded') ||
          result.fills?.some((fill) => fill.status === 'confirmed_unrecorded')
            ? $t('dex.confirmedPendingSync')
            : result.outcome === 'partial'
            ? $t('dex.routePartiallyFilled', {
                count: result.tx_ids?.length || result.result?.tx_ids?.length || 0,
                amount: result.result?.remaining || result.remaining || 0,
              })
            : result.outcome === 'filled_and_placed' || result.outcome === 'placed'
            ? $t('dex.fillThenPlaceSubmitted', {
                count: result.created_orders?.length || 0,
              })
            : $t('dex.routeFilled');
      } else {
        const result = await api.submitCreate(currentWallet.id, createPin, pendingPrepare.intent_id);
        info = $t('dex.orderCreated');
      }
      pendingPrepare = null;
      createPin = '';
      resetForm();
      await Promise.all([loadMyOrders(), refreshBook(), loadTrades(), loadBalance(), loadPairs()]);
    } catch (e) {
      error = e.message;
    } finally {
      submitting = false;
    }
  }

  // ---- Fill order ----
  async function prepareFill(order) {
    fillTarget = order;
    fillPrepare = null;
    loading = true;
    error = '';
    try {
      fillPrepare = await api.prepareFill(currentWallet.id, signer, order.escrow_addr);
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function submitFill() {
    if (!fillPrepare || !fillTarget) return;
    if (!getToken()) {
      error = $t('common.sessionExpired');
      return;
    }
    if (!fillPin) {
      error = $t('transfer.pinRequired');
      return;
    }
    submitting = true;
    error = '';
    try {
      const result = await api.submitFill(currentWallet.id, fillPin, fillPrepare.intent_id);
      info = result.recorded ? $t('dex.orderFilled') : $t('dex.confirmedPendingSync');
      fillPrepare = null;
      fillTarget = null;
      fillPin = '';
      await Promise.all([loadMyOrders(), refreshBook(), loadTrades(), loadBalance(), loadPairs()]);
    } catch (e) {
      error = e.message;
    } finally {
      submitting = false;
    }
  }

  // ---- Cancel order ----
  async function prepareCancel(order) {
    await prepareCancelMany([order]);
  }

  async function prepareCancelMany(orders) {
    const activeTargets = (orders || []).filter((order) => order?.status === 'active');
    if (!activeTargets.length) return;
    cancelTarget = activeTargets[0];
    cancelPrepare = null;
    cancelBatch = activeTargets;
    cancelPrepares = [];
    loading = true;
    error = '';
    try {
      const prepared = [];
      for (const target of activeTargets) {
        const prepare = await api.prepareCancel(currentWallet.id, target.escrow_addr);
        prepared.push({ order: target, prepare });
      }
      cancelTarget = activeTargets[0];
      cancelPrepares = prepared;
      cancelPrepare = prepared[0]?.prepare || null;
    } catch (e) {
      cancelBatch = [];
      cancelPrepares = [];
      cancelPrepare = null;
      cancelTarget = null;
      error = e.message;
    } finally {
      loading = false;
    }
  }

  async function submitCancel() {
    if (!cancelTarget) return;
    if (!getToken()) {
      error = $t('common.sessionExpired');
      return;
    }
    if (!cancelPin) {
      error = $t('transfer.pinRequired');
      return;
    }
    submitting = true;
    error = '';
    try {
      const items = cancelPrepares.length ? cancelPrepares : [{ order: cancelTarget, prepare: cancelPrepare }];
      let recorded = 0;
      const results = [];
      for (const item of items) {
        const result = await api.submitCancel(currentWallet.id, cancelPin, item.prepare.intent_id);
        if (result.recorded) recorded += 1;
        results.push({
          escrow_address: item.order?.escrow_addr,
          tx_id: result.tx_id,
          confirmed_round: result.confirmed_round,
          recovered_amount: result.recovered_amount,
          recorded: result.recorded,
        });
      }
      info =
        items.length > 1
          ? $t('dex.ordersCancelled', { count: recorded, total: items.length })
          : recorded
            ? $t('dex.orderCancelled')
            : $t('dex.confirmedPendingSync');
      cancelPrepare = null;
      cancelTarget = null;
      cancelBatch = [];
      cancelPrepares = [];
      cancelPin = '';
      await Promise.all([loadMyOrders(), refreshBook(), loadTrades(), loadBalance(), loadPairs()]);
    } catch (e) {
      error = e.message;
      await Promise.all([loadMyOrders(), refreshBook(), loadTrades(), loadBalance(), loadPairs()]);
    } finally {
      submitting = false;
    }
  }

  // ---- Helpers ----
  function resetForm() {
    orderPrice = '';
    orderAmount = '';
    splitCount = 1;
    routeCandidates = null;
    routeError = '';
  }

  function switchSide(newSide) {
    side = newSide;
    routeCandidates = null;
    routeError = '';
  }

  // ---- Formatting helpers ----
  function fmtPrice(micro) {
    return formatMicroPrice(micro, currentBaseDecimals(), currentQuoteDecimals());
  }

  function currentBaseDecimals() {
    return baseAssetId == 0 ? 6 : (baseAssetMeta?.decimals ?? 6);
  }

  function currentQuoteDecimals() {
    return Number(quoteAssetId) == 0 ? 6 : (quoteAssetMeta?.decimals ?? 6);
  }

  function fmtBaseAmount(raw) {
    if (raw == null) return '—';
    return formatRawAmount(raw, currentBaseDecimals());
  }

  function fmtAssetAmount(raw, assetId) {
    if (raw == null) return '—';
    if (Number(assetId) === Number(baseAssetId)) return formatRawAmount(raw, currentBaseDecimals());
    if (Number(assetId) === Number(quoteAssetId)) return formatRawAmount(raw, currentQuoteDecimals());
    return formatDisplayAmount(raw);
  }

  function fmtAssetLabel(id) {
    return formatAssetIdLabel(id);
  }

  function statusColor(status) {
    return dexStatusClass(status);
  }

  function statusLabel(status) {
    return dexStatusLabel(status, $t);
  }

  function relTime(unixSec) {
    return relativeTimeLabel(unixSec, $t);
  }

  // Sparkline data from recent confirmed trades (price over time).
  $: chartTrades = (() => {
    if (!trades?.length) return [];
    const range = CHART_RANGES.find((item) => item.id === chartRange);
    if (!range || range.seconds == null) return trades;
    const cutoff = Math.floor(Date.now() / 1000) - range.seconds;
    return trades.filter((tr) => Number(tr.timestamp || 0) >= cutoff);
  })();

  $: sparklineData = (() => {
    if (!chartTrades.length) return null;
    const prices = [...chartTrades].reverse().map((tr) => Number(tr.price));
    if (prices.length === 1) {
      const price = prices[0];
      return {
        points: '50,15',
        single: true,
        isUp: true,
        min: price,
        max: price,
      };
    }
    const min = Math.min(...prices);
    const max = Math.max(...prices);
    const range = max - min || 1;
    const w = 100;
    const h = 30;
    const step = w / (prices.length - 1);
    const points = prices
      .map((p, i) => {
        const x = i * step;
        const y = h - ((p - min) / range) * h;
        return `${x.toFixed(1)},${y.toFixed(1)}`;
      })
      .join(' ');
    // Direction: green if latest >= earliest, red otherwise.
    const isUp = prices[prices.length - 1] >= prices[0];
    return { points, isUp, min, max };
  })();
  // Backwards-compat alias used in the template.
  $: sparklinePoints = sparklineData?.points ?? '';
  $: sparklineColor = sparklineData?.isUp ? '#00d4aa' : '#ef4444';

  // Derived stats from orderbook
  $: bestBid = book?.bids?.[0]?.price ?? null;
  $: bestAsk = book?.asks?.[0]?.price ?? null;
  $: maxAskAmount = book?.asks ? Math.max(...book.asks.map((l) => Number(l.amount)), 0) : 0;
  $: maxBidAmount = book?.bids ? Math.max(...book.bids.map((l) => Number(l.amount)), 0) : 0;
  $: syntheticAsks = book?.synthetic_asks || [];
  $: syntheticBids = book?.synthetic_bids || [];
  $: maxSyntheticAskAmount = syntheticAsks.length
    ? Math.max(...syntheticAsks.map((l) => Number(l.amount)), 0)
    : 0;
  $: maxSyntheticBidAmount = syntheticBids.length
    ? Math.max(...syntheticBids.map((l) => Number(l.amount)), 0)
    : 0;
  $: activeOrders = myOrders.filter((o) => o.status === 'active');
  $: historyOrders = myOrders.filter((o) => o.status !== 'active');

  // Canonical key of the currently-selected pair, for highlighting in the
  // popular-pairs sidebar. asset_a is the smaller id (ALGO=0 sorts first).
  $: selectedPairKey = quoteAssetId
    ? (Number(baseAssetId) < Number(quoteAssetId)
        ? `${baseAssetId}:${quoteAssetId}`
        : `${quoteAssetId}:${baseAssetId}`)
    : '';

  // Load book + my orders when wallet or quoteAssetId becomes available.
  $: currentBookPairKey = quoteAssetId ? `${baseAssetId}:${quoteAssetId}` : '';
  $: if (currentBookPairKey && currentBookPairKey !== lastLoadedBookPairKey) {
    lastLoadedBookPairKey = currentBookPairKey;
    refreshBook();
  }
  $: if ($pendingOrderLinkPayload) loadOrderLink($pendingOrderLinkPayload);
  $: if (signer) loadMyOrders();
  $: if (signer) loadBalance();
</script>

<section class="dex-view">
  {#if error}
    <div class="mb-4 rounded-md border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-300">
      {error}
    </div>
  {/if}
  {#if info}
    <div class="mb-4 rounded-md border border-green-500/30 bg-green-500/10 px-4 py-3 text-sm text-green-300">
      {info}
    </div>
  {/if}
  <OrderLinkPanel
    {orderLinkLoading}
    {orderLinkDetail}
    {orderLinkError}
    {orderLinkUi}
    {loading}
    {submitting}
    {signer}
    {copyOpenedOrderLink}
    {prepareFill}
    {prepareCancel}
    {statusLabel}
    {fmtAssetLabel}
    {fmtAssetAmount}
  />

  {#if !signer}
    <div class="card flex items-center justify-center py-16 text-center">
      <p class="text-sm text-gray-500">{$t('dex.noWalletSelected')}</p>
    </div>
  {:else}
    <MarketHeader
      {quoteAssetMeta}
      {baseAssetMeta}
      {book}
      {quoteAssetId}
      {bookLoading}
      {bestBid}
      {bestAsk}
      {onQuoteSelect}
      {onBaseSelect}
      {swapPair}
      {fmtPrice}
    />

    <CommunityMarketsPanel
      {communityMarkets}
      {communityMarketsLoading}
      {communityMarketError}
      {loadCommunityMarkets}
      {selectCommunityPair}
      {marketBadgeClass}
      {marketWarning}
      {communityPairMatchesCurrent}
    />

    <MobileTradingTabs bind:mobileTab />

    <PopularPairsNav
      variant="mobile"
      {pairs}
      {pairsLoading}
      {selectedPairKey}
      {selectPair}
      {pairKey}
      {pairLabelFor}
      {fmtPrice}
    />

    <!-- ====== Main Trading Layout (Phase 6) ====== -->
    <div class="trading-grid grid grid-cols-1 gap-4 lg:grid-cols-5 xl:grid-cols-6">
      <PopularPairsNav
        variant="sidebar"
        {pairs}
        {pairsLoading}
        {selectedPairKey}
        {selectPair}
        {pairKey}
        {pairLabelFor}
        {fmtPrice}
      />

      <!-- ====== Center Column: Chart + Orderbook + Trades ====== -->
      <div class="space-y-4 lg:col-span-3 xl:col-span-3">
        <!-- Price Chart (Phase 6.2 / Phase 4 placeholder) -->
        <ChartPanel
          {mobileTab}
          {pairLabel}
          chartRanges={CHART_RANGES}
          bind:chartRange
          {sparklineData}
          {sparklinePoints}
          {sparklineColor}
          {quoteAssetId}
          {fmtPrice}
        />

        <!-- Orderbook (Phase 2) -->
        <OrderBookPanel
          {book}
          {bookLoading}
          {quoteAssetId}
          {pairLabel}
          {mobileTab}
          {syntheticAsks}
          {syntheticBids}
          {maxAskAmount}
          {maxBidAmount}
          {maxSyntheticAskAmount}
          {maxSyntheticBidAmount}
          {fmtPrice}
          {fmtBaseAmount}
          {clickLevel}
          {handleLevelKeydown}
          widthForDepth={depthWidth}
        />

        <!-- Recent Trades (Phase 4) -->
        <RecentTradesPanel
          {mobileTab}
          {trades}
          {loadTrades}
          {relTime}
          {fmtPrice}
          {fmtBaseAmount}
        />
      </div>

      <!-- ====== Right Column: Order Form + My Orders ====== -->
      <div class="space-y-4 lg:col-span-2 xl:col-span-2 lg:block {mobileTab === 'order' ? 'block' : 'hidden'}">
        <!-- Order Form (Phase 3) -->
        <OrderFormPanel
          bind:side
          {quoteAssetId}
          {quoteLabel}
          {baseLabel}
          bind:orderPrice
          bind:orderAmount
          {orderTotal}
          expiryPresets={EXPIRY_PRESETS}
          {expiryPreset}
          {expireRounds}
          {estimatedExpiry}
          bind:splitCount
          bind:iocMode
          bind:fillThenPlace
          {effectiveSplitCount}
          {perOrderAmount}
          {estimatedEscrowCost}
          {routeCandidates}
          {routeLoading}
          {routeError}
          {commonQuoteStatus}
          {loading}
          {switchSide}
          {setMaxAmount}
          {selectExpiryPreset}
          {onExpireRoundsInput}
          {refreshRouteCandidates}
          {prepareCreate}
          {fmtAssetAmount}
        />

        <!-- My Orders (Phase 5) -->
        <MyOrdersPanel
          bind:ordersTab
          {myOrdersLoading}
          {activeOrders}
          {historyOrders}
          {loading}
          {submitting}
          {fmtAssetLabel}
          {fmtPrice}
          {fmtAssetAmount}
          {statusColor}
          {statusLabel}
          {splitChildCount}
          {splitProgress}
          {splitActiveOrders}
          {isFirstActiveSplitChild}
          {prepareFill}
          {prepareCancel}
          {prepareCancelMany}
          {copyOrderLink}
        />
      </div>
    </div>

    <!-- ====== Modals ====== -->

    <CreateConfirmModal
      {pendingPrepare}
      bind:createPin
      {submitting}
      {focusTrap}
      {closeModals}
      {submitCreate}
      {fmtAssetAmount}
    />

    <FillConfirmModal
      {fillTarget}
      {fillPrepare}
      bind:fillPin
      {submitting}
      {focusTrap}
      {closeModals}
      {submitFill}
    />

    <CancelConfirmModal
      {cancelTarget}
      {cancelPrepare}
      {cancelBatch}
      bind:cancelPin
      {submitting}
      {focusTrap}
      {closeModals}
      {submitCancel}
      {cancelBatchRecoverableAlgo}
      {cancelBatchRecoverableAssets}
      {fmtAssetAmount}
    />
  {/if}
</section>

<style>
  @media (max-width: 1023px) {
    .trading-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
