<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import QRCode from 'qrcode';
  import { t } from '../i18n/index.js';

  const dispatch = createEventDispatcher();

  export let wallet;

  let qrDataUrl = '';
  let copied = false;
  let error = '';
  let copiedTimer;

  onMount(async () => {
    try {
      qrDataUrl = await QRCode.toDataURL(wallet.first_address, {
        width: 280,
        margin: 2,
        color: {
          dark: '#0d1117',
          light: '#ffffff',
        },
      });
    } catch (e) {
      error = e.message;
    }
  });

  function close() {
    dispatch('close');
  }

  async function copyAddress() {
    try {
      await navigator.clipboard.writeText(wallet.first_address);
    } catch (_) {
      const textarea = document.createElement('textarea');
      textarea.value = wallet.first_address;
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand('copy');
      document.body.removeChild(textarea);
    }
    copied = true;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copied = false), 2000);
  }
</script>

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
  on:click|self={close}
  on:keydown={(event) => event.key === 'Escape' && close()}
  role="presentation"
>
  <div
    class="card max-h-[90vh] w-full max-w-md overflow-y-auto text-center"
    role="dialog"
    aria-modal="true"
    aria-labelledby="receive-title"
  >
    <div class="mb-5 flex items-center justify-between text-left">
      <div>
        <h3 id="receive-title" class="text-lg font-semibold text-gray-100">{$t('receive.title')}</h3>
        <p class="mt-1 text-xs text-gray-500">{wallet.name}</p>
      </div>
      <button class="rounded p-1 text-gray-500 hover:text-gray-300" on:click={close} aria-label={$t('common.cancel')}>
        <svg class="h-5 w-5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
          <path stroke-linecap="round" d="M6 18 18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    {#if qrDataUrl}
      <div class="mx-auto mb-5 w-fit rounded-xl bg-white p-3">
        <img class="h-64 w-64" src={qrDataUrl} alt={$t('receive.qrAlt')} />
      </div>
    {:else if error}
      <div class="mb-5 rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm text-red-400">{error}</div>
    {:else}
      <div class="mx-auto mb-5 h-64 w-64 animate-pulse rounded-xl bg-gray-800"></div>
    {/if}

    <button
      class="w-full break-all rounded-lg border border-gray-700 bg-gray-900/50 p-3 font-mono text-xs text-gray-300 hover:border-algo-500/50"
      on:click={copyAddress}
    >
      {wallet.first_address}
    </button>
    <p class="mt-2 text-xs {copied ? 'text-green-400' : 'text-gray-500'}">
      {copied ? $t('receive.copied') : $t('receive.copyHint')}
    </p>

    <div class="mt-5 rounded-lg border border-yellow-500/20 bg-yellow-500/5 px-4 py-3 text-left text-xs text-yellow-200">
      {$t('receive.optInNotice')}
    </div>
  </div>
</div>
