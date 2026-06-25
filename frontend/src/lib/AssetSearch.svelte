<script>
  import { createEventDispatcher, tick } from 'svelte';
  import { api } from '../api.js';
  import { t } from '../i18n/index.js';

  const dispatch = createEventDispatcher();

  /**
   * Props:
   *  - placeholder: input placeholder text
   *  - allowManualEntry: when true, users can still type a raw asset ID
   *    even if the indexer is unavailable.
   */
  export let placeholder = '';
  export let allowManualEntry = true;
  export let selectedAsset = null; // { id, name, unit, decimals }

  let query = '';
  let results = [];
  let loading = false;
  let showDropdown = false;
  let error = '';
  let searchTimer = null;
  let inputEl;
  let appliedSelectionKey = '';

  // Display the selected asset's label when set externally.
  $: if (selectedAsset) {
    const selectionKey = `${selectedAsset.id}:${selectedAsset.unit || ''}:${selectedAsset.name || ''}`;
    if (selectionKey !== appliedSelectionKey) {
      appliedSelectionKey = selectionKey;
      query = selectedAsset.unit || selectedAsset.name || `#${selectedAsset.id}`;
    }
  }

  function onInput(event) {
    query = event.target.value;
    appliedSelectionKey = '';
    selectedAsset = null;
    dispatch('select', null);
    clearTimeout(searchTimer);

    // If the user typed a pure number, treat it as a manual asset ID.
    if (/^\d+$/.test(query.trim())) {
      const id = Number(query.trim());
      if (id > 0) {
        // Emit as a manual entry immediately; also try to fetch metadata.
        showDropdown = false;
        results = [];
        dispatch('select', { id, name: '', unit: '', decimals: 6, manual: true });
        fetchMetadata(id);
      }
      return;
    }

    if (query.trim().length < 2) {
      results = [];
      showDropdown = false;
      return;
    }

    // Debounce search by 300ms.
    searchTimer = setTimeout(doSearch, 300);
  }

  async function doSearch() {
    loading = true;
    error = '';
    try {
      results = await api.searchAssets(query.trim());
      showDropdown = true;
    } catch (e) {
      error = e.message;
      results = [];
      showDropdown = allowManualEntry;
    } finally {
      loading = false;
    }
  }

  async function fetchMetadata(id) {
    try {
      const meta = await api.getAssetMetadata(id);
      dispatch('select', {
        id: meta.id,
        name: meta.name,
        unit: meta.unit,
        decimals: meta.decimals,
        manual: true,
      });
    } catch {
      // Metadata fetch failed; keep the manual ID entry.
    }
  }

  function selectResult(result) {
    selectedAsset = result;
    query = result.unit || result.name || `#${result.id}`;
    showDropdown = false;
    dispatch('select', {
      id: result.id,
      name: result.name,
      unit: result.unit,
      decimals: result.decimals,
    });
  }

  function onBlur() {
    // Delay to allow click events on dropdown items to fire first.
    setTimeout(() => (showDropdown = false), 200);
  }

  function onFocus() {
    if (results.length > 0) showDropdown = true;
  }
</script>

<div class="relative">
  <input
    bind:this={inputEl}
    class="input"
    type="text"
    {placeholder}
    value={query}
    on:input={onInput}
    on:focus={onFocus}
    on:blur={onBlur}
    autocomplete="off"
  />

  {#if loading}
    <div class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2">
      <svg class="h-4 w-4 animate-spin text-gray-500" fill="none" viewBox="0 0 24 24">
        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 0 1 8-8V0C5.4 0 0 5.4 0 12h4z" />
      </svg>
    </div>
  {/if}

  {#if showDropdown && (results.length > 0 || error)}
    <div class="absolute z-30 mt-1 max-h-64 w-full overflow-y-auto rounded-lg border border-gray-700 bg-gray-900 shadow-xl">
      {#if error}
        <div class="px-4 py-3 text-xs text-yellow-400">
          {$t('assets.searchUnavailable')}
          {#if allowManualEntry}
            <br />{$t('assets.searchManualHint')}
          {/if}
        </div>
      {/if}
      {#each results as result}
        <button
          type="button"
          class="flex w-full items-center justify-between gap-3 px-4 py-2.5 text-left hover:bg-gray-800"
          on:click={() => selectResult(result)}
        >
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="font-medium text-gray-200">{result.unit || '—'}</span>
              {#if result.name}
                <span class="truncate text-xs text-gray-500">{result.name}</span>
              {/if}
            </div>
          </div>
          <span class="shrink-0 font-mono text-xs text-gray-600">#{result.id}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>
