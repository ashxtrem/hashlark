<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { LoaderCircle, Sparkles, X } from '@lucide/svelte';
  import type { Update } from '@tauri-apps/plugin-updater';
  import { isTauri } from '$lib/platform';
  import { app } from '$lib/state/app.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';

  let update = $state.raw<Update | null>(null);
  let installing = $state(false);
  let dismissed = $state(false);

  // Only the desktop app updates itself, and only if the user allows it.
  onMount(async () => {
    if (!isTauri() || !app.settings?.updates.check_for_updates) return;
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      update = await check();
    } catch (e) {
      console.warn('update check failed', e);
    }
  });

  async function install() {
    if (!update) return;
    installing = true;
    try {
      await update.downloadAndInstall();
      const { relaunch } = await import('@tauri-apps/plugin-process');
      await relaunch();
    } catch (e) {
      toasts.error(`The update failed: ${errorMessage(e)}`);
      installing = false;
    }
  }
</script>

{#if update && !dismissed}
  <div class="flex items-center gap-3 border-b border-line bg-accent-soft px-6 py-2 text-sm" role="status">
    <Sparkles size={16} class="text-accent" />
    <p class="flex-1">Hashlark {update.version} is available.</p>
    <button class="btn-primary py-1" disabled={installing} onclick={install}>
      {#if installing}<LoaderCircle size={14} class="animate-spin" /> Installing…{:else}Install and restart{/if}
    </button>
    <button class="icon-btn size-7" aria-label="Later" onclick={() => (dismissed = true)}><X size={15} /></button>
  </div>
{/if}
