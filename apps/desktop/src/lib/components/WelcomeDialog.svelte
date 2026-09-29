<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import Modal from './Modal.svelte';
  import { app } from '$lib/state/app.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';

  let saving = $state(false);

  async function finish() {
    if (!app.settings) return;
    saving = true;
    try {
      await app.saveSettings({ ...app.settings, ui: { ...app.settings.ui, first_run_done: true } });
    } catch (e) {
      toasts.error(errorMessage(e));
    } finally {
      saving = false;
    }
  }
</script>

<Modal open={app.settings !== null && !app.settings.ui.first_run_done} title="Welcome to Hashlark" required>
  <div class="space-y-3">
    <p>
      Hashlark searches many torrent indexers at once and hands what you pick to your torrent
      client.
    </p>
    <ul class="list-disc space-y-1.5 pl-5 text-muted">
      <li>
        It starts with <strong class="text-fg">legal sources only</strong>, such as the Internet Archive. You
        can add other providers under <em>Providers</em>.
      </li>
      <li>Hashlark hosts no content and doesn't download anything itself.</li>
      <li>
        <strong class="text-fg">No telemetry.</strong> Hashlark only contacts the providers you enable,
        definition repositories you add, and GitHub to check for updates (you can turn this off).
      </li>
    </ul>
    <p class="rounded-lg bg-warn-soft px-3 py-2 text-warn">
      You are responsible for complying with the law and copyright where you live.
    </p>
  </div>
  {#snippet actions()}
    <button class="btn-primary" disabled={saving} onclick={finish}>I understand, get started</button>
  {/snippet}
</Modal>
