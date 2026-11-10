<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { Copy, File, FileDown, Folder, LoaderCircle } from '@lucide/svelte';
  import Modal from './Modal.svelte';
  import type { CreatedTorrent } from '$lib/api/client';
  import { copyText, isTauri, pickFile, pickFolder } from '$lib/platform';
  import { app } from '$lib/state/app.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';
  import { torrents } from '$lib/state/torrents.svelte';

  let path = $state('');
  let name = $state('');
  let trackerText = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let created = $state<CreatedTorrent | null>(null);

  $effect(() => {
    if (!torrents.createOpen) return;
    path = '';
    name = '';
    trackerText = '';
    error = null;
    created = null;
  });

  function close() {
    torrents.createOpen = false;
  }

  async function pick(kind: 'file' | 'folder') {
    const picked = kind === 'file' ? await pickFile() : await pickFolder();
    if (picked) path = picked;
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      const trackers = trackerText
        .split(/\s+/)
        .map((t) => t.trim())
        .filter(Boolean);
      created = await app.client.createTorrent({ path: path.trim(), name: name.trim() || null, trackers });
      await torrents.refresh();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }

  async function copyMagnet() {
    if (!created) return;
    await copyText(created.magnet);
    toasts.success('Magnet link copied');
  }

  async function saveFile() {
    if (!created) return;
    try {
      const blob = await app.client.torrentFile(created.torrent.id);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${created.torrent.name}.torrent`;
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 10_000);
    } catch (err) {
      toasts.error(errorMessage(err));
    }
  }
</script>

<Modal open={torrents.createOpen} title="Create torrent" onclose={close}>
  {#if created}
    <p>
      {created.already ? 'This torrent is already in Downloads.' : 'Created and seeding now.'}
      Share the magnet link or the .torrent file.
    </p>
    <p class="mt-3 font-medium break-words">{created.torrent.name}</p>
    <code class="mt-2 block max-h-24 overflow-y-auto rounded-lg bg-surface-2 p-2 font-mono text-xs break-all">
      {created.magnet}
    </code>
  {:else}
    <form id="create-torrent" class="space-y-4" onsubmit={create}>
      <div>
        <label class="label" for="create-path">File or folder to share</label>
        <input id="create-path" class="input" required bind:value={path} placeholder={isTauri() ? '' : '/data/share/folder'} />
        {#if isTauri()}
          <div class="mt-2 flex gap-2">
            <button type="button" class="btn-outline" onclick={() => pick('file')}><File size={15} /> Choose file</button>
            <button type="button" class="btn-outline" onclick={() => pick('folder')}><Folder size={15} /> Choose folder</button>
          </div>
        {:else}
          <p class="hint">A path on the server, inside its allowed folders.</p>
        {/if}
      </div>
      <div>
        <label class="label" for="create-name">Name <span class="font-normal text-muted">(optional)</span></label>
        <input id="create-name" class="input" bind:value={name} />
      </div>
      <div>
        <label class="label" for="create-trackers">Trackers <span class="font-normal text-muted">(one per line, optional)</span></label>
        <textarea id="create-trackers" class="input font-mono text-xs" rows="4" bind:value={trackerText}
          placeholder="udp://tracker.example.org:1337/announce"></textarea>
      </div>
      {#if error}
        <p class="text-err" role="alert">{error}</p>
      {/if}
    </form>
  {/if}
  {#snippet actions()}
    {#if created}
      <button class="btn-outline" onclick={saveFile}><FileDown size={15} /> Save .torrent</button>
      <button class="btn-outline" onclick={copyMagnet}><Copy size={15} /> Copy magnet</button>
      <button class="btn-primary" onclick={close}>Done</button>
    {:else}
      <button class="btn-ghost" onclick={close}>Cancel</button>
      <button class="btn-primary" type="submit" form="create-torrent" disabled={busy || !path.trim()}>
        {#if busy}<LoaderCircle size={15} class="animate-spin" />{/if} Create
      </button>
    {/if}
  {/snippet}
</Modal>
