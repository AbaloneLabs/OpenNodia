<script>
  export let loading = false;
  export let error = '';
  export let stale = false;
  export let source = '';
  export let message = '';
</script>

{#if loading || error || stale || message}
  <div
    class="rounded-lg border px-3 py-2 text-sm {error
      ? 'border-red-500/30 bg-red-500/10 text-red-300'
      : stale
      ? 'border-yellow-500/30 bg-yellow-500/10 text-yellow-300'
      : loading
      ? 'border-gray-600 bg-gray-800/60 text-gray-300'
      : 'border-green-500/20 bg-green-500/10 text-green-300'}"
  >
    <div class="flex flex-wrap items-center gap-2">
      {#if source}
        <span class="rounded-full bg-black/20 px-2 py-0.5 text-[10px] uppercase tracking-wide">
          {source}
        </span>
      {/if}
      <span>
        {#if error}
          {error}
        {:else if stale}
          {message || 'Quote is stale. Refresh before signing.'}
        {:else if loading}
          {message || 'Loading quote...'}
        {:else}
          {message}
        {/if}
      </span>
    </div>
  </div>
{/if}
