<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { Copy, Download } from '@lucide/svelte';
  import Modal from './Modal.svelte';
  import { copyText } from '$lib/platform';
  import { app } from '$lib/state/app.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';
  import { torrents } from '$lib/state/torrents.svelte';
  import { ui } from '$lib/state/ui.svelte';

  let turningOn = $state(false);

  async function copy() {
    if (!ui.noHandlerMagnet) return;
    await copyText(ui.noHandlerMagnet);
    toasts.success('Magnet link copied');
    ui.noHandlerMagnet = null;
  }

  /** Turns the built-in client on and adds the link that had no handler. */
  async function turnOn() {
    const magnet = ui.noHandlerMagnet;
    if (!magnet || !app.settings) return;
    turningOn = true;
    try {
      await app.saveSettings({ ...app.settings, torrents: { ...app.settings.torrents, enabled: true } });
    } catch (e) {
      toasts.error(`Could not turn on the built-in client: ${errorMessage(e)}`);
      return;
    } finally {
      turningOn = false;
    }
    // Keep the dialog (and the link) if the add fails, so it can be retried.
    if (await torrents.add({ source: { magnet } })) ui.noHandlerMagnet = null;
  }
</script>

<Modal
  open={ui.noHandlerMagnet !== null}
  title="No torrent client found"
  onclose={() => (ui.noHandlerMagnet = null)}
>
  <p>
    No app on this computer is set up to open magnet links, so Hashlark can't hand this download
    over.
  </p>
  {#if torrents.enabled}
    <p class="mt-3">
      Install a torrent client (for example qBittorrent, Transmission or Deluge), then try again. You
      can also copy the magnet link and paste it into a client yourself.
    </p>
  {:else}
    <p class="mt-3">
      Hashlark has a torrent client of its own. Turn it on to download this right here, or install
      another client (for example qBittorrent, Transmission or Deluge) and try again.
    </p>
  {/if}
  {#snippet actions()}
    <button class="btn-ghost" onclick={() => (ui.noHandlerMagnet = null)}>Close</button>
    <button class={torrents.enabled ? 'btn-primary' : 'btn-outline'} onclick={copy}><Copy size={16} /> Copy magnet link</button>
    {#if !torrents.enabled}
      <button class="btn-primary" disabled={turningOn} onclick={turnOn}>
        <Download size={16} /> Turn on the built-in client
      </button>
    {/if}
  {/snippet}
</Modal>
