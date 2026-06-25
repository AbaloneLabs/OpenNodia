<script>
  import { t } from '../i18n/index.js';
  import { activeView } from '../nav.js';

  export let network = 'local';
  export let algoPrice = null;

  const navItems = [
    { id: 'node', label: 'nav.node', icon: 'node' },
    { id: 'wallets', label: 'nav.wallets', icon: 'wallets' },
    { id: 'assets', label: 'nav.assets', icon: 'assets' },
    { id: 'dex', label: 'nav.dexTrade', icon: 'dex' },
    { id: 'lp', label: 'nav.lpTrade', icon: 'lp', child: true },
    { id: 'asa', label: 'nav.asaIssue', icon: 'asa', child: true },
    { id: 'settings', label: 'common.settings', icon: 'settings' },
  ];

  const networkStyles = {
    mainnet: { bg: 'bg-orange-500/10', text: 'text-orange-400', dot: 'bg-orange-400', label: 'Mainnet' },
    testnet: { bg: 'bg-blue-500/10', text: 'text-blue-400', dot: 'bg-blue-400', label: 'Testnet' },
    betanet: { bg: 'bg-purple-500/10', text: 'text-purple-400', dot: 'bg-purple-400', label: 'Betanet' },
    local: { bg: 'bg-gray-500/10', text: 'text-gray-400', dot: 'bg-gray-400', label: 'Local' },
  };

  $: netStyle = networkStyles[network] || networkStyles.local;
  $: priceText = algoPrice != null ? `$${algoPrice.toFixed(4)}` : null;
</script>

<!-- Desktop sidebar -->
<aside class="hidden w-60 shrink-0 flex-col border-r border-gray-700/50 bg-surface lg:flex">
  <div class="px-5 py-4">
    <div class="flex items-center gap-2.5">
      <img src="/opennodia-logo.svg" alt="OpenNodia" class="h-8 w-8" />
      <span class="text-lg font-bold text-gray-100">OpenNodia</span>
    </div>
    <!-- Network badge + price below title -->
    <div class="mt-2 flex flex-col gap-1.5">
      <span class="flex w-fit items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-semibold {netStyle.bg} {netStyle.text}">
        <span class="h-1.5 w-1.5 rounded-full {netStyle.dot}"></span>
        {netStyle.label}
      </span>
      {#if priceText}
        <div class="flex items-center gap-1.5 text-[15px] text-gray-500">
          <span class="font-medium text-gray-400">ALGO</span>
          <span class="font-mono font-semibold text-gray-300">{priceText}</span>
          <span class="text-gray-600">/ USDC</span>
        </div>
      {/if}
    </div>
  </div>

  <nav class="flex-1 space-y-1 px-3 py-2">
    {#each navItems as item (item.id)}
      <button
        class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors
          {item.child ? 'ml-6 w-[calc(100%-1.5rem)] text-xs' : ''}
          {$activeView === item.id
          ? 'bg-algo-500/10 text-algo-400'
          : 'text-gray-400 hover:bg-gray-700/30 hover:text-gray-200'}"
        on:click={() => activeView.set(item.id)}
      >
        <span class="flex h-5 w-5 items-center justify-center">
          {#if item.icon === 'node'}
            <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
            </svg>
          {:else if item.icon === 'wallets'}
            <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" d="M21 12a2.25 2.25 0 00-2.25-2.25H15a3 3 0 11-6 0H5.25A2.25 2.25 0 003 12m18 0v6a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 18v-6m18 0V9M3 12V9m18 0a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 9m18 0V6a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 6v3" />
            </svg>
          {:else if item.icon === 'assets'}
            <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
            </svg>
          {:else if item.icon === 'dex'}
            <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" />
            </svg>
          {:else if item.icon === 'lp'}
            <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" d="M4 17c3.5-5 6.5-5 10 0M4 7c3.5 5 6.5 5 10 0M17 6h3m-3 6h3m-3 6h3" />
            </svg>
          {:else if item.icon === 'asa'}
            <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4m3-5h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2Z" />
            </svg>
          {:else if item.icon === 'settings'}
            <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
          {/if}
        </span>
        {$t(item.label)}
      </button>
    {/each}
  </nav>

  <div class="px-5 py-4">
    <p class="text-xs text-gray-600">v0.1.0 · pre-alpha</p>
  </div>
</aside>

<!-- Mobile bottom tab bar -->
<nav class="fixed bottom-0 left-0 right-0 z-30 flex border-t border-gray-700/50 bg-surface-dark/95 backdrop-blur lg:hidden">
  {#each navItems as item (item.id)}
    <button
      class="flex flex-1 flex-col items-center gap-0.5 py-2.5 text-[10px] font-medium transition-colors
        {$activeView === item.id ? 'text-algo-400' : 'text-gray-500'}"
      on:click={() => activeView.set(item.id)}
    >
      <span class="flex h-5 w-5 items-center justify-center">
        {#if item.icon === 'node'}
          <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
          </svg>
        {:else if item.icon === 'wallets'}
          <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M21 12a2.25 2.25 0 00-2.25-2.25H15a3 3 0 11-6 0H5.25A2.25 2.25 0 003 12m18 0v6a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 18v-6m18 0V9M3 12V9m18 0a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 9m18 0V6a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 6v3" />
          </svg>
        {:else if item.icon === 'assets'}
          <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
          </svg>
        {:else if item.icon === 'dex'}
          <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" />
          </svg>
        {:else if item.icon === 'lp'}
          <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M4 17c3.5-5 6.5-5 10 0M4 7c3.5 5 6.5 5 10 0M17 6h3m-3 6h3m-3 6h3" />
          </svg>
        {:else if item.icon === 'asa'}
          <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 4v16m8-8H4m3-5h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2Z" />
          </svg>
        {:else if item.icon === 'settings'}
          <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="1.8" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        {/if}
      </span>
      <span>{$t(item.label)}</span>
    </button>
  {/each}
</nav>
