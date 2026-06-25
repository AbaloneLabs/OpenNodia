<script>
  import { api } from '../api.js';
  import { t } from '../i18n/index.js';

  // --- PIN change section ---
  let currentPin = '';
  let newPin = '';
  let confirmPin = '';
  let pinLoading = false;
  let pinError = '';
  let pinSuccess = '';

  async function changePin() {
    pinError = '';
    pinSuccess = '';

    if (newPin.length < 4) {
      pinError = $t('settings.pinTooShort');
      return;
    }
    if (newPin !== confirmPin) {
      pinError = $t('settings.pinsDoNotMatch');
      return;
    }

    pinLoading = true;
    try {
      await api.changePin(currentPin, newPin);
      pinSuccess = $t('settings.pinChanged');
      currentPin = '';
      newPin = '';
      confirmPin = '';
    } catch (e) {
      pinError = e.message;
    } finally {
      pinLoading = false;
    }
  }
</script>

<section>
  <h2 class="mb-6 text-lg font-semibold text-gray-200">{$t('common.settings')}</h2>

  <div class="max-w-lg">
    <!-- PIN change section -->
    <div class="card">
      <h4 class="mb-3 text-sm font-medium text-gray-300">{$t('settings.changePin')}</h4>
      <div class="space-y-3">
        <div>
          <label class="label" for="currentPin">{$t('settings.currentPin')}</label>
          <input id="currentPin" type="password" bind:value={currentPin} class="input" placeholder={$t('settings.currentPin')} autocomplete="current-password" />
        </div>
        <div>
          <label class="label" for="newPin">{$t('settings.newPin')}</label>
          <input id="newPin" type="password" bind:value={newPin} class="input" placeholder={$t('settings.newPin')} autocomplete="new-password" />
        </div>
        <div>
          <label class="label" for="confirmPin">{$t('settings.confirmNewPin')}</label>
          <input id="confirmPin" type="password" bind:value={confirmPin} class="input" placeholder={$t('settings.confirmNewPin')} autocomplete="new-password" />
        </div>
      </div>

      {#if pinError}
        <div class="mt-3 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-2 text-sm text-red-400">{pinError}</div>
      {/if}
      {#if pinSuccess}
        <div class="mt-3 rounded-lg border border-green-500/30 bg-green-500/10 px-4 py-2 text-sm text-green-400">{pinSuccess}</div>
      {/if}

      <button class="btn-primary mt-4 w-full" on:click={changePin} disabled={pinLoading || !currentPin || !newPin}>
        {#if pinLoading}{$t('settings.changing')}{:else}{$t('settings.changePin')}{/if}
      </button>
    </div>

    <div class="mt-6 text-center">
      <p class="text-xs text-gray-600">{$t('settings.walletHint')}</p>
    </div>
  </div>
</section>
