<script>
  import { t } from '../../i18n/index.js';

  export let pool = null;
  export let lpOptInPin = '';
  export let loadingLpOptIn = false;
  export let currentWallet = null;
  export let lpOptInResult = null;
  export let submitLpAssetOptIn = () => {};
  export let assetLabel = (assetId) => `#${assetId}`;
  export let formatAsset = (raw) => String(raw ?? '0');
  export let formatBps = (bps) => `${bps ?? 0} bps`;
  export let formatLp = (raw) => String(raw ?? '0');
  export let scaledRateToBps = (rate) => rate ?? 0;
</script>

{#if pool?.pool}
  <section class="card">
    <div class="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
      <div>
        <h3 class="text-lg font-semibold text-gray-100">{$t('lpTrade.poolDetail')}</h3>
        <p class="mt-1 break-all font-mono text-xs text-gray-500">{pool.pool.pool_id}</p>
      </div>
      <span class="w-fit rounded-full bg-gray-700/50 px-3 py-1 text-xs text-gray-300">
        {pool.pool.source} · {pool.pool.swap_supported ? $t('dex.sourceReady') : $t('lpTrade.quoteOnly')}
      </span>
    </div>

    <div class="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
      <div class="rounded-lg bg-surface-dark p-3">
        <div class="text-xs text-gray-500">{$t('lpTrade.pair')}</div>
        <div class="mt-1 font-mono text-sm text-gray-100">{assetLabel(pool.pool.asset_0)} / {assetLabel(pool.pool.asset_1)}</div>
      </div>
      <div class="rounded-lg bg-surface-dark p-3">
        <div class="text-xs text-gray-500">{$t('lpTrade.reserves')}</div>
        <div class="mt-1 font-mono text-sm text-gray-100">{formatAsset(pool.pool.reserve_0, pool.pool.asset_0)} / {formatAsset(pool.pool.reserve_1, pool.pool.asset_1)}</div>
      </div>
      <div class="rounded-lg bg-surface-dark p-3">
        <div class="text-xs text-gray-500">{$t('lpTrade.lpAsset')}</div>
        <div class="mt-1 font-mono text-sm text-gray-100">{pool.pool.lp_asset_id ? assetLabel(pool.pool.lp_asset_id) : '—'}</div>
      </div>
      <div class="rounded-lg bg-surface-dark p-3">
        <div class="text-xs text-gray-500">{$t('lpTrade.totalLp')}</div>
        <div class="mt-1 font-mono text-sm text-gray-100">{formatLp(pool.pool.total_lp_supply)}</div>
      </div>
      <div class="rounded-lg bg-surface-dark p-3">
        <div class="text-xs text-gray-500">{$t('lpTrade.sourceRound')}</div>
        <div class="mt-1 font-mono text-sm text-gray-100">{pool.pool.source_round}</div>
      </div>
    </div>

    <div class="mt-4 rounded-lg bg-surface-dark p-3">
      <div class="text-xs text-gray-500">{$t('lpTrade.appAddress')}</div>
      <div class="mt-1 break-all font-mono text-sm text-gray-100">{pool.pool.app_address}</div>
      {#if pool.pool.status_note}
        <div class="mt-2 text-xs text-gray-500">{pool.pool.status_note}</div>
      {/if}
      {#if pool.pool.folks_backed && pool.pool.folks}
        <div class="mt-3 rounded-lg border border-algo-500/20 bg-algo-500/10 p-3 text-xs text-algo-100">
          <div class="font-semibold text-algo-300">{$t('lpTrade.folksBacked')}</div>
          <div class="mt-3 grid gap-2 md:grid-cols-2">
            <div class="rounded bg-black/10 p-2">
              <div class="text-gray-400">{$t('lpTrade.folksUnderlying')}</div>
              <div class="mt-1 font-mono text-algo-100">{assetLabel(pool.pool.folks.underlying_0)} / {assetLabel(pool.pool.folks.underlying_1)}</div>
            </div>
            <div class="rounded bg-black/10 p-2">
              <div class="text-gray-400">{$t('lpTrade.folksSwapFee')}</div>
              <div class="mt-1 font-mono text-algo-100">{pool.pool.fee_bps} bps</div>
            </div>
            <div class="rounded bg-black/10 p-2">
              <div class="text-gray-400">{$t('lpTrade.folksDepositApr')}</div>
              <div class="mt-1 font-mono text-algo-100">
                {formatBps(scaledRateToBps(pool.pool.folks.deposit_interest_rate_0))} / {formatBps(scaledRateToBps(pool.pool.folks.deposit_interest_rate_1))}
              </div>
            </div>
            <div class="rounded bg-black/10 p-2">
              <div class="text-gray-400">{$t('lpTrade.folksUtilization')}</div>
              <div class="mt-1 font-mono text-algo-100">
                {pool.pool.folks.utilization_available ? `${formatBps(pool.pool.folks.utilization_bps_0)} / ${formatBps(pool.pool.folks.utilization_bps_1)}` : '—'}
              </div>
            </div>
          </div>
          <div class="mt-3 grid gap-2 md:grid-cols-3">
            <div>
              <div class="text-gray-400">{$t('lpTrade.folksRedeemable')}</div>
              <div class="mt-1 font-mono text-algo-100">{formatAsset(pool.pool.folks.redeem_available_0, pool.pool.folks.underlying_0)} / {formatAsset(pool.pool.folks.redeem_available_1, pool.pool.folks.underlying_1)}</div>
            </div>
            <div>
              <div class="text-gray-400">{$t('lpTrade.folksTotalDeposit')}</div>
              <div class="mt-1 font-mono text-algo-100">{formatAsset(pool.pool.folks.total_deposit_0, pool.pool.folks.underlying_0)} / {formatAsset(pool.pool.folks.total_deposit_1, pool.pool.folks.underlying_1)}</div>
            </div>
            <div>
              <div class="text-gray-400">{$t('lpTrade.folksTotalBorrowed')}</div>
              <div class="mt-1 font-mono text-algo-100">{formatAsset(pool.pool.folks.total_borrowed_0, pool.pool.folks.underlying_0)} / {formatAsset(pool.pool.folks.total_borrowed_1, pool.pool.folks.underlying_1)}</div>
            </div>
          </div>
          <div class="mt-1 text-gray-400">{pool.pool.folks.utilization_note}</div>
          <div class="mt-1 text-gray-400">{pool.pool.folks.risk_note}</div>
        </div>
      {/if}
    </div>

    {#if pool.pool.lp_asset_id}
      <div class="mt-4 rounded-lg border border-yellow-500/20 bg-yellow-500/10 p-3 text-sm">
        <p class="text-yellow-300">{$t('lpTrade.lpOptInHint', { lpAsset: pool.pool.lp_asset_id })}</p>
        <div class="mt-3 flex flex-col gap-3 sm:flex-row sm:items-end">
          <label class="block sm:max-w-xs">
            <span class="label">{$t('transfer.pin')}</span>
            <input class="input mt-1" type="password" bind:value={lpOptInPin} autocomplete="current-password" />
          </label>
          <button class="btn-secondary" type="button" on:click={submitLpAssetOptIn} disabled={loadingLpOptIn || !currentWallet}>
            {loadingLpOptIn ? $t('lpTrade.submitting') : $t('lpTrade.lpOptInAction')}
          </button>
        </div>
        {#if lpOptInResult}
          <p class="mt-2 text-green-300">{$t('lpTrade.lpOptInSuccess')}</p>
        {/if}
      </div>
    {/if}
  </section>
{/if}
