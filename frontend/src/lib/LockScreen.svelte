<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import { api } from '../api.js';
  import { t } from '../i18n/index.js';

  const dispatch = createEventDispatcher();

  // status.setup_complete tells us whether a PIN already exists.
  export let status = null;

  let mode = 'login'; // 'login' | 'create' | 'confirm'
  let pin = '';
  let confirmPin = '';
  let loading = false;
  let error = '';
  let pinInput;

  onMount(() => pinInput?.focus());

  $: needsSetup = status && !status.setup_complete;

  // Auto-switch to create mode if no PIN is set yet.
  $: if (needsSetup && mode === 'login') {
    mode = 'create';
  }

  async function submit() {
    error = '';

    if (mode === 'create' || mode === 'confirm') {
      if (pin.length < 4) {
        error = $t('lock.pinTooShort');
        return;
      }
      if (mode === 'create') {
        mode = 'confirm';
        confirmPin = '';
        return;
      }
      // confirm mode
      if (pin !== confirmPin) {
        error = $t('lock.pinsDoNotMatch');
        return;
      }
      // First-time setup: PIN only (no mnemonic here).
      loading = true;
      try {
        const resp = await api.setup(pin);
        dispatch('unlocked', resp);
      } catch (e) {
        error = e.message;
        mode = 'create';
        pin = '';
        confirmPin = '';
      } finally {
        loading = false;
      }
      return;
    }

    // login mode
    loading = true;
    try {
      const resp = await api.login(pin);
      dispatch('unlocked', resp);
    } catch (e) {
      error = e.message;
      pin = '';
    } finally {
      loading = false;
    }
  }

  function onKeydown(e) {
    if (e.key === 'Enter') submit();
  }

  function reset() {
    mode = needsSetup ? 'create' : 'login';
    pin = '';
    confirmPin = '';
    error = '';
  }

  $: title =
    mode === 'create' ? $t('lock.createPin') : mode === 'confirm' ? $t('lock.confirmPin') : $t('lock.enterPin');

  $: subtitle =
    mode === 'create'
      ? $t('lock.createSubtitle')
      : mode === 'confirm'
        ? $t('lock.confirmSubtitle')
        : $t('lock.enterSubtitle');
</script>

<div class="flex min-h-screen items-center justify-center px-4">
  <div class="w-full max-w-sm">
    <!-- Logo -->
    <div class="mb-8 text-center">
      <img src="/opennodia-logo.svg" alt="OpenNodia" class="mx-auto mb-4 h-16 w-16" />
      <h1 class="text-2xl font-bold text-gray-100">OpenNodia</h1>
      <p class="mt-1 text-sm text-gray-500">{subtitle}</p>
    </div>

    <div class="card">
      <div>
        <label class="label" for="pin">{title}</label>
        <input
          bind:this={pinInput}
          id="pin"
          type="password"
          bind:value={pin}
          on:keydown={onKeydown}
          class="input text-center text-2xl tracking-widest"
          placeholder="••••"
          autocomplete={mode === 'login' ? 'current-password' : 'new-password'}
        />
      </div>

      {#if mode === 'confirm'}
        <div class="mt-4">
          <label class="label" for="confirmPin">{$t('lock.confirmPin')}</label>
          <input
            id="confirmPin"
            type="password"
            bind:value={confirmPin}
            on:keydown={onKeydown}
            class="input text-center text-2xl tracking-widest"
            placeholder="••••"
            autocomplete="new-password"
          />
        </div>
      {/if}

      {#if error}
        <div class="mt-4 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-2.5 text-sm text-red-400">
          {error}
        </div>
      {/if}

      <button class="btn-primary mt-6 w-full" on:click={submit} disabled={loading || !pin}>
        {#if loading}
          <svg class="h-4 w-4 animate-spin" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" stroke-dasharray="40" stroke-linecap="round" />
          </svg>
          {mode === 'login' ? $t('lock.unlocking') : $t('lock.settingUp')}
        {:else}
          {mode === 'login' ? $t('common.unlock') : mode === 'confirm' ? $t('common.confirm') : $t('common.continue')}
        {/if}
      </button>

      {#if mode === 'confirm'}
        <button class="btn-secondary mt-3 w-full" on:click={() => { mode = 'create'; pin = ''; confirmPin = ''; error = ''; }}>
          {$t('common.back')}
        </button>
      {/if}
    </div>

  </div>
</div>
