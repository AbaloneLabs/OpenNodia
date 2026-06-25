<script>
  import { locale, locales, setLocale, t } from '../i18n/index.js';
  import { onMount } from 'svelte';

  let open = false;
  let buttonEl;
  let menuEl;

  function toggle() {
    open = !open;
  }

  function choose(code) {
    setLocale(code);
    open = false;
  }

  function handleWindowClick(e) {
    if (!open) return;
    if (menuEl && !menuEl.contains(e.target) && buttonEl && !buttonEl.contains(e.target)) {
      open = false;
    }
  }

  function handleKeydown(e) {
    if (e.key === 'Escape') open = false;
  }

  onMount(() => {
    window.addEventListener('click', handleWindowClick);
    window.addEventListener('keydown', handleKeydown);
    return () => {
      window.removeEventListener('click', handleWindowClick);
      window.removeEventListener('keydown', handleKeydown);
    };
  });

  $: current = locales.find((l) => l.code === $locale) || locales[0];
</script>

<div class="lang-selector">
  <button
    type="button"
    class="lang-button"
    bind:this={buttonEl}
    on:click={toggle}
    aria-haspopup="listbox"
    aria-expanded={open}
    title={$t('header.selectLanguage')}
  >
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="10"/>
      <line x1="2" y1="12" x2="22" y2="12"/>
      <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>
    </svg>
    <span class="lang-short">{current.short}</span>
  </button>

  {#if open}
    <div class="lang-menu" bind:this={menuEl} role="listbox">
      {#each locales as l (l.code)}
        <button
          type="button"
          class="lang-option"
          class:active={l.code === $locale}
          on:click={() => choose(l.code)}
          role="option"
          aria-selected={l.code === $locale}
        >
          <span class="lang-label">{l.label}</span>
          {#if l.code === $locale}
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="20 6 9 17 4 12"/>
            </svg>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .lang-selector {
    position: relative;
    display: inline-block;
  }

  .lang-button {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 10px;
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 6px;
    color: #9ca3af;
    cursor: pointer;
    font-size: 0.8125rem;
    font-weight: 600;
    transition: all 0.15s ease;
  }

  .lang-button:hover {
    background: #1c2330;
    border-color: #484f58;
    color: #e5e7eb;
  }

  .lang-short {
    letter-spacing: 0.5px;
  }

  .lang-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    min-width: 160px;
    background: #374151;
    border: 1px solid #4b5563;
    border-radius: 8px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.7);
    padding: 4px;
    z-index: 200;
    animation: fadeIn 0.1s ease;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .lang-option {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 8px 10px;
    background: transparent;
    border: none;
    border-radius: 5px;
    color: #e5e7eb;
    cursor: pointer;
    font-size: 0.875rem;
    text-align: left;
    transition: all 0.1s ease;
  }

  .lang-option:hover {
    background: #4b5563;
    color: #ffffff;
  }

  .lang-option.active {
    color: #00d4aa;
    font-weight: 600;
  }

  .lang-label {
    flex: 1;
  }
</style>
