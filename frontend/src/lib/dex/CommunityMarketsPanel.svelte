<script>
  import { t } from '../../i18n/index.js';

  export let communityMarkets = [];
  export let communityMarketsLoading = false;
  export let communityMarketError = '';
  export let loadCommunityMarkets = () => {};
  export let selectCommunityPair = () => {};
  export let marketBadgeClass = () => '';
  export let marketWarning = () => '';
  export let communityPairMatchesCurrent = () => false;
</script>

{#if communityMarketsLoading || communityMarketError || communityMarkets.length > 0}
  <section class="mb-4 rounded-md border border-gray-800 bg-gray-900/50 p-3">
    <div class="mb-2 flex items-center justify-between gap-3">
      <div>
        <h3 class="text-xs font-semibold uppercase tracking-wider text-gray-400">{$t('dex.communityMarkets')}</h3>
        <p class="mt-0.5 text-[11px] text-gray-600">{$t('dex.communityMarketsHint')}</p>
      </div>
      <button
        type="button"
        class="min-h-[36px] rounded-md border border-gray-700 px-3 py-1 text-xs text-gray-300 hover:border-gray-600 hover:text-gray-100"
        on:click={loadCommunityMarkets}
      >
        {$t('common.refresh')}
      </button>
    </div>

    {#if communityMarketError}
      <div class="rounded-md border border-yellow-500/30 bg-yellow-500/10 px-3 py-2 text-xs text-yellow-200">
        {communityMarketError}
      </div>
    {:else if communityMarketsLoading && communityMarkets.length === 0}
      <div class="py-3 text-center text-xs text-gray-600">{$t('dex.loading')}</div>
    {:else}
      <div class="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
        {#each communityMarkets as market}
          <button
            type="button"
            class="min-h-[92px] rounded-md border border-gray-800 bg-gray-950/70 p-3 text-left transition hover:border-gray-700"
            on:click={() => selectCommunityPair(market)}
          >
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0">
                <div class="truncate text-sm font-semibold text-gray-100">{market.name}</div>
                <div class="truncate font-mono text-[10px] text-gray-600">{market.id}</div>
              </div>
              <span class="shrink-0 rounded-md border px-2 py-0.5 text-[10px] {marketBadgeClass(market)}">
                {market.official ? $t('dex.officialMarket') : $t('dex.unverifiedMarket')}
              </span>
            </div>
            <div class="mt-2 flex flex-wrap gap-1">
              {#each (market.pairs || []).slice(0, 4) as pair}
                <span
                  class="rounded border px-1.5 py-0.5 font-mono text-[10px] {communityPairMatchesCurrent(pair)
                    ? 'border-indigo-500/50 bg-indigo-500/10 text-indigo-200'
                    : pair.official
                      ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-200'
                      : 'border-gray-700 bg-gray-900 text-gray-400'}"
                >
                  {pair.display}
                </span>
              {/each}
            </div>
            {#if marketWarning(market)}
              <div class="mt-2 line-clamp-2 text-[11px] leading-snug text-yellow-300">
                {marketWarning(market)}
              </div>
            {:else if market.migration_notice}
              <div class="mt-2 line-clamp-2 text-[11px] leading-snug text-blue-300">
                {market.migration_notice}
              </div>
            {/if}
          </button>
        {/each}
      </div>
    {/if}
  </section>
{/if}
