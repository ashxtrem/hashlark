<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { Copy } from '@lucide/svelte';
  import Modal from './Modal.svelte';
  import { copyText } from '$lib/platform';
  import { toasts } from '$lib/state/toasts.svelte';
  import { ui } from '$lib/state/ui.svelte';

  async function copy() {
    if (!ui.noHandlerMagnet) return;
    await copyText(ui.noHandlerMagnet);
    toasts.success('Magnet link copied');
    ui.noHandlerMagnet = null;
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
  <p class="mt-3">
    Install a torrent client (for example qBittorrent, Transmission or Deluge), then try again. You
    can also copy the magnet link and paste it into a client yourself.
  </p>
  {#snippet actions()}
    <button class="btn-ghost" onclick={() => (ui.noHandlerMagnet = null)}>Close</button>
    <button class="btn-primary" onclick={copy}><Copy size={16} /> Copy magnet link</button>
  {/snippet}
</Modal>
