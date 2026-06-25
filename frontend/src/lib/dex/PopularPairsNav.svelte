<script>
  import { t } from '../../i18n/index.js';

  export let pairs = [];
  export let pairsLoading = false;
  export let selectedPairKey = '';
  export let selectPair = () => {};
  export let pairKey = (pair) => `${pair.asset_a}:${pair.asset_b}`;
  export let pairLabelFor = (pair) => `${pair.asset_b}/${pair.asset_a}`;
  export let fmtPrice = (value) => String(value ?? '—');
  export let variant = 'mobile';
</script>

{#if variant === 'mobile' && pairs.length > 0}
  <div class="-mx-1 mb-3 flex gap-2 overflow-x-auto px-1 pb-1 xl:hidden">
    {#each pairs as pair}
      <button
        type="button"
        class="min-h-[44px] min-w-[7rem] max-w-[9rem] shrink-0 rounded-md border px-3 py-2 text-left transition {selectedPairKey === pairKey(pair)
          ? 'border-indigo-500 bg-indigo-600/20 text-indigo-100'
          : 'border-gray-700 bg-gray-900/70 text-gray-300'}"
        on:click={() => selectPair(pair)}
      >
        <div class="truncate font-mono text-xs font-semibold">{pairLabelFor(pair)}</div>
        <div class="mt-0.5 font-mono text-[10px] text-gray-500">
          {pair.last_price != null ? fmtPrice(pair.last_price) : '—'}
        </div>
      </button>
    {/each}
  </div>
{/if}

{#if variant === 'sidebar'}
  <aside class="pairs-sidebar hidden xl:col-span-1 xl:block">
    <div class="card sticky top-4 p-3">
      <h3 class="mb-2 px-1 text-xs font-semibold uppercase tracking-wider text-gray-400">
        {$t('dex.pairsTitle')}
      </h3>
      {#if pairsLoading && pairs.length === 0}
        <div class="py-4 text-center text-[11px] text-gray-600">{$t('dex.loading')}</div>
      {:else if pairs.length === 0}
        <div class="py-4 text-center text-[11px] text-gray-600">{$t('dex.noActivePairs')}</div>
      {:else}
        <div class="space-y-0.5">
          {#each pairs as pair}
            <button
              type="button"
              class="pair-row flex w-full items-center justify-between rounded-md px-2 py-1.5 text-left transition {selectedPairKey === pairKey(pair)
                ? 'bg-indigo-600/20 text-indigo-200'
                : 'text-gray-300 hover:bg-gray-800'}"
              on:click={() => selectPair(pair)}
            >
              <span class="font-mono text-xs font-medium">{pairLabelFor(pair)}</span>
              <span class="font-mono text-[10px] text-gray-500">
                {pair.last_price != null ? fmtPrice(pair.last_price) : '—'}
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </aside>
{/if}
