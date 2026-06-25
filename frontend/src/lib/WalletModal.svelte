<script>
  import { createEventDispatcher } from 'svelte';
  import { api } from '../api.js';
  import { t } from '../i18n/index.js';

  const dispatch = createEventDispatcher();

  export let mode = 'create'; // 'create' | 'import'

  let name = '';
  let mnemonic = '';
  let pin = '';
  let loading = false;
  let error = '';
  let success = '';

  async function submit() {
    error = '';
    success = '';

    if (!name.trim()) {
      error = $t('walletModal.nameRequired');
      return;
    }

    if (mode === 'import') {
      const words = mnemonic.trim().split(/\s+/);
      if (words.length !== 25) {
        error = $t('walletModal.mnemonicLength', { count: words.length });
        return;
      }
    }

    if (!pin) {
      error = $t('walletModal.pinRequired');
      return;
    }

    loading = true;
    try {
      let wallet;
      if (mode === 'create') {
        wallet = await api.createWallet(name.trim(), pin);
      } else {
        wallet = await api.importWallet(name.trim(), mnemonic.trim(), pin);
      }
      success = $t('walletModal.success', { name: wallet.name, addr: wallet.first_address.slice(0, 8) });
      dispatch('created', wallet);
      setTimeout(() => dispatch('close'), 1500);
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  function close() {
    dispatch('close');
  }
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 px-4"
  on:click|self={close}
  on:keydown={(e) => e.key === 'Escape' && close()}
  role="button"
  tabindex="-1"
>
  <div class="w-full max-w-lg card">
    <div class="mb-4 flex items-center justify-between">
      <h3 class="text-lg font-semibold text-gray-100">
        {mode === 'create' ? $t('walletModal.createTitle') : $t('walletModal.importTitle')}
      </h3>
      <button class="text-gray-500 hover:text-gray-300" on:click={close} aria-label={$t('common.cancel')}>
        <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    <div class="space-y-4">
      <!-- Wallet name -->
      <div>
        <label class="label" for="walletName">{$t('walletModal.walletName')}</label>
        <input
          id="walletName"
          class="input"
          bind:value={name}
          placeholder={$t('walletModal.namePlaceholder')}
        />
      </div>

      {#if mode === 'import'}
        <!-- Mnemonic -->
        <div>
          <label class="label" for="mnemonic">{$t('walletModal.mnemonicLabel')}</label>
          <p class="mb-2 text-xs text-gray-500">
            {$t('walletModal.mnemonicHint')}
          </p>
          <textarea
            id="mnemonic"
            class="input font-mono text-sm"
            rows="4"
            bind:value={mnemonic}
            placeholder={$t('walletModal.mnemonicPlaceholder')}
            spellcheck="false"
          ></textarea>
        </div>
      {:else}
        <div class="rounded-lg border border-algo-500/20 bg-algo-500/5 px-4 py-3">
          <p class="text-xs text-gray-400">
            {$t('walletModal.createHint')}
          </p>
        </div>
      {/if}

      <!-- PIN -->
      <div>
        <label class="label" for="walletPin">{$t('walletModal.pinLabel')}</label>
        <input
          id="walletPin"
          type="password"
          class="input"
          bind:value={pin}
          placeholder={$t('walletModal.pinPlaceholder')}
          autocomplete="current-password"
        />
      </div>

      {#if error}
        <div class="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-2 text-sm text-red-400">
          {error}
        </div>
      {/if}
      {#if success}
        <div class="rounded-lg border border-green-500/30 bg-green-500/10 px-4 py-2 text-sm text-green-400">
          {success}
        </div>
      {/if}

      <button class="btn-primary w-full" on:click={submit} disabled={loading || !name.trim() || !pin}>
        {#if loading}
          {mode === 'create' ? $t('walletModal.creating') : $t('walletModal.importing')}
        {:else}
          {mode === 'create' ? $t('walletModal.createBtn') : $t('walletModal.importBtn')}
        {/if}
      </button>
    </div>
  </div>
</div>
