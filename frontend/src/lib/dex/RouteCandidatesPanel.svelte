<script>
  import { t } from '../../i18n/index.js';
  import QuoteStatus from '../QuoteStatus.svelte';

  export let routeCandidates = null;
  export let routeLoading = false;
  export let routeError = '';
  export let orderAmount = '';
  export let orderPrice = '';
  export let commonQuoteStatus = { state: 'idle', stale: false, source: '', message: '' };
  export let refreshRouteCandidates = () => {};
  export let fmtAssetAmount = (raw) => String(raw ?? '—');
</script>

<div class="mb-4 rounded-lg border border-gray-700 bg-gray-900/50 p-3 text-xs text-gray-400">
  <div class="mb-2 flex items-center justify-between gap-2">
    <span class="font-semibold text-gray-200">{$t('dex.sourcePreview')}</span>
  </div>
  <div class="flex flex-wrap gap-2">
    <span class="rounded-full bg-algo-500/10 px-2 py-0.5 text-algo-300">
      {$t('dex.orderbookSource')} · {$t('dex.sourceReady')}
    </span>
    <span class="rounded-full bg-gray-700/60 px-2 py-0.5 text-gray-400">
      {$t('dex.openNodiaPoolSource')} · {$t('dex.sourceNotConnected')}
    </span>
    <span class="rounded-full bg-gray-700/60 px-2 py-0.5 text-gray-400">
      {$t('dex.tinymanSource')} · {$t('dex.sourceNotConnected')}
    </span>
    <span class="rounded-full bg-gray-700/60 px-2 py-0.5 text-gray-400">
      {$t('dex.pactSource')} · {$t('dex.sourceNotConnected')}
    </span>
  </div>
  <div class="mt-3 rounded-lg border border-gray-800 bg-gray-950/40 p-3">
    <div class="mb-2 flex items-center justify-between gap-2">
      <div class="font-semibold text-gray-200">{$t('dex.routeCandidates')}</div>
      <button
        type="button"
        class="rounded-md border border-gray-700 px-2 py-1 text-[11px] text-gray-300 hover:bg-gray-800 disabled:opacity-50"
        on:click={refreshRouteCandidates}
        disabled={routeLoading || !orderAmount || !orderPrice}
      >
        {routeLoading ? $t('common.loading') : $t('dex.refreshRoutes')}
      </button>
    </div>
    {#if routeError}
      <div class="rounded border border-red-500/30 bg-red-500/10 px-2 py-1.5 text-[11px] text-red-300">
        {routeError}
      </div>
    {:else if routeCandidates?.candidates?.length}
      <div class="space-y-2">
        {#each routeCandidates.candidates.slice(0, 4) as candidate}
          <div class="rounded-md border {routeCandidates.selected?.route_hash === candidate.route_hash ? 'border-algo-500/60 bg-algo-500/10' : 'border-gray-800 bg-gray-900/70'} p-2">
            <div class="flex items-center justify-between gap-2">
              <div class="min-w-0">
                <div class="truncate text-xs font-medium text-gray-200">
                  {candidate.source_label}
                  {#if routeCandidates.selected?.route_hash === candidate.route_hash}
                    <span class="ml-1 text-[10px] text-algo-300">best</span>
                  {/if}
                </div>
                <div class="mt-0.5 flex flex-wrap gap-1 text-[10px] text-gray-500">
                  <span class="rounded bg-gray-800 px-1.5 py-0.5">
                    {candidate.source_type === 'split'
                      ? $t('dex.routeCandidateSplit')
                      : candidate.virtual_orderbook
                        ? $t('dex.routeCandidateVirtual')
                        : $t('dex.routeCandidateRealBook')}
                  </span>
                  <span class="rounded px-1.5 py-0.5 {candidate.executable ? 'bg-green-500/10 text-green-300' : 'bg-yellow-500/10 text-yellow-300'}">
                    {candidate.executable ? $t('dex.routeCandidateExecutable') : $t('dex.routeCandidateQuoteOnly')}
                  </span>
                </div>
              </div>
              <div class="text-right font-mono text-xs text-gray-100">
                {fmtAssetAmount(candidate.amount_out, routeCandidates.asset_out)}
              </div>
            </div>
            <div class="mt-1 grid grid-cols-2 gap-2 text-[11px] text-gray-500">
              <div>
                {$t('dex.routeCandidateMin')}: <span class="font-mono text-gray-300">{fmtAssetAmount(candidate.minimum_out, routeCandidates.asset_out)}</span>
              </div>
              <div class="text-right">
                fee {candidate.lp_fee_bps ?? candidate.fee_bps} bps · impact {candidate.price_impact_bps} bps
              </div>
            </div>
            <div class="mt-1 text-[11px] text-gray-500">
              network fee: <span class="font-mono text-gray-300">{candidate.network_fee_microalgo || 0}</span> µALGO
            </div>
            {#if candidate.split_legs?.length}
              <div class="mt-1 space-y-1 rounded bg-gray-950/50 px-2 py-1.5 text-[11px] text-gray-500">
                <div class="text-gray-400">{$t('dex.routeSplitLegs')}</div>
                {#each candidate.split_legs as leg}
                  <div class="flex justify-between gap-2">
                    <span class="min-w-0 truncate">{leg.source_label} #{leg.app_id}</span>
                    <span class="shrink-0 font-mono text-gray-300">
                      {fmtAssetAmount(leg.amount_in, leg.asset_in)} → {fmtAssetAmount(leg.minimum_out, leg.asset_out)}
                    </span>
                  </div>
                {/each}
              </div>
            {/if}
            {#if candidate.remaining_input > 0}
              <div class="mt-1 text-[11px] text-yellow-300">
                {$t('dex.routeCandidatePartial', {
                  amount: fmtAssetAmount(candidate.remaining_input, routeCandidates.asset_in),
                })}
              </div>
            {/if}
            <p class="mt-1 text-[11px] text-gray-500">{candidate.note}</p>
          </div>
        {/each}
      </div>
    {:else if routeCandidates}
      <div class="rounded border border-gray-800 bg-gray-900/70 px-2 py-2 text-[11px] text-gray-500">
        {$t('dex.noRouteCandidates')}
      </div>
    {/if}
    {#if routeCandidates?.warnings?.length}
      <div class="mt-2 rounded border border-yellow-500/20 bg-yellow-500/10 px-2 py-1.5 text-[11px] text-yellow-200">
        <div class="font-medium">{$t('dex.routeWarnings')}</div>
        <ul class="mt-1 list-disc space-y-0.5 pl-4">
          {#each routeCandidates.warnings.slice(0, 3) as warning}
            <li>{warning}</li>
          {/each}
        </ul>
      </div>
    {/if}
  </div>
  <div class="mt-3">
    <QuoteStatus
      loading={commonQuoteStatus.state === 'loading'}
      error={commonQuoteStatus.state === 'error' ? commonQuoteStatus.message : ''}
      stale={commonQuoteStatus.stale}
      source={commonQuoteStatus.source}
      message={commonQuoteStatus.message}
    />
  </div>
</div>
