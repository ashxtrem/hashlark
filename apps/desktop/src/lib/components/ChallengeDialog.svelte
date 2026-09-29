<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { ExternalLink, LoaderCircle } from '@lucide/svelte';
  import Modal from './Modal.svelte';
  import { finishChallenge, isTauri, openChallenge } from '$lib/platform';
  import { app } from '$lib/state/app.svelte';
  import { providers } from '$lib/state/providers.svelte';
  import { search } from '$lib/state/search.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';
  import { ui } from '$lib/state/ui.svelte';

  let opened = $state(false);
  let finishing = $state(false);

  $effect(() => {
    if (!ui.challenge) opened = false;
  });

  async function open() {
    if (!ui.challenge) return;
    try {
      await openChallenge(ui.challenge.url);
      opened = true;
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  async function finish() {
    const c = ui.challenge;
    if (!c) return;
    finishing = true;
    try {
      const count = await finishChallenge(c.providerId, c.url);
      toasts.success(`${c.name} unlocked (${count} cookie${count === 1 ? '' : 's'} saved)`);
      ui.challenge = null;
      await providers.load();
      if (search.shownQuery) search.run(app.client);
    } catch (e) {
      toasts.error(errorMessage(e));
    } finally {
      finishing = false;
    }
  }
</script>

<Modal open={ui.challenge !== null} title="Browser check needed" onclose={() => (ui.challenge = null)}>
  {#if ui.challenge}
    <p>
      <strong>{ui.challenge.name}</strong> shows a “checking your browser” page that a person has to
      pass. Hashlark doesn't solve these automatically.
    </p>
    {#if isTauri()}
      <ol class="mt-3 list-decimal space-y-1.5 pl-5 text-muted">
        <li>Open the site in a separate window and complete the check there.</li>
        <li>When the site's normal page appears, come back and click <em>Done</em>.</li>
      </ol>
      <p class="mt-3 text-xs text-muted">
        Hashlark keeps the site's cookies for this provider only, until they expire.
      </p>
    {:else}
      <p class="mt-3 text-muted">
        This has to be done in the Hashlark desktop app. On a headless server, route this provider
        through Jackett/Prowlarr with FlareSolverr, or through a proxy the site accepts.
      </p>
    {/if}
  {/if}
  {#snippet actions()}
    <button class="btn-ghost" onclick={() => (ui.challenge = null)}>Cancel</button>
    {#if isTauri()}
      <button class="btn-outline" onclick={open}><ExternalLink size={15} /> {opened ? 'Show site again' : 'Open site'}</button>
      <button class="btn-primary" disabled={!opened || finishing} onclick={finish}>
        {#if finishing}<LoaderCircle size={15} class="animate-spin" />{/if} Done
      </button>
    {/if}
  {/snippet}
</Modal>
