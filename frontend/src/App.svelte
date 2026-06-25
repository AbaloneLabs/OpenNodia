<script>
  import { onMount } from 'svelte';
  import { api, setToken } from './api.js';
  import LockScreen from './lib/LockScreen.svelte';
  import AppShell from './lib/AppShell.svelte';

  let view = 'loading'; // loading | lock | dashboard
  let status = null;

  async function checkStatus() {
    try {
      status = await api.getStatus();
      if (!status.setup_complete) {
        setToken(null);
        view = 'lock';
        return;
      }
      try {
        const session = await api.getSession();
        setToken(Boolean(session?.expires_at));
        view = 'dashboard';
      } catch (_) {
        setToken(null);
        view = 'lock';
      }
    } catch (e) {
      // Backend unreachable — still show lock screen.
      setToken(null);
      view = 'lock';
    }
  }

  function onUnlocked(resp) {
    setToken(resp?.expires_at || true);
    view = 'dashboard';
  }

  async function onLogout() {
    try {
      await api.logout();
    } catch (_) {}
    setToken(null);
    view = 'lock';
  }

  onMount(checkStatus);
</script>

{#if view === 'loading'}
  <div class="flex min-h-screen items-center justify-center">
    <div class="animate-pulse text-algo-500">
      <svg class="h-12 w-12" viewBox="0 0 100 100" fill="none" stroke="currentColor" stroke-width="4">
        <circle cx="50" cy="50" r="40" stroke-dasharray="60" stroke-linecap="round" />
      </svg>
    </div>
  </div>
{:else if view === 'lock'}
  <LockScreen {status} on:unlocked={(e) => onUnlocked(e.detail)} />
{:else if view === 'dashboard'}
  <AppShell {status} on:logout={onLogout} />
{/if}
