<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import Modal from './Modal.svelte';
  import { torrents } from '$lib/state/torrents.svelte';

  let busy = $state(false);

  async function answer(ok: boolean) {
    busy = true;
    await torrents.answerNotice(ok);
    busy = false;
  }
</script>

<Modal open={torrents.noticeOpen} title="Before your first download" onclose={() => answer(false)}>
  <p>
    Downloading shares pieces with other people and shows your IP address to them. Only download what you have
    the right to.
  </p>
  <p class="mt-3 text-muted">You'll only see this once.</p>
  {#snippet actions()}
    <button class="btn-ghost" disabled={busy} onclick={() => answer(false)}>Cancel</button>
    <button class="btn-primary" disabled={busy} onclick={() => answer(true)}>I understand</button>
  {/snippet}
</Modal>
