<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { FolderOpen } from '@lucide/svelte';
  import FileTree from './FileTree.svelte';
  import Modal from './Modal.svelte';
  import { bytes } from '$lib/format';
  import { isTauri, pickFolder } from '$lib/platform';
  import { app } from '$lib/state/app.svelte';
  import { torrents } from '$lib/state/torrents.svelte';
  import { fileTree } from '$lib/torrents';

  let prompt = $derived(torrents.addPrompt);
  let folder = $state('');
  let paused = $state(false);
  let selected = $state(new Set<number>());
  let tree = $derived(prompt?.files ? fileTree(prompt.files) : []);
  let total = $derived(
    prompt?.files?.filter((f) => selected.has(f.index)).reduce((s, f) => s + f.size, 0) ?? 0,
  );

  // Fresh choices for every prompt.
  $effect(() => {
    if (!prompt) return;
    folder = prompt.req.save_dir ?? '';
    paused = prompt.req.paused ?? false;
    selected = new Set(prompt.files?.map((f) => f.index) ?? []);
  });

  function toggle(indices: number[], checked: boolean) {
    const next = new Set(selected);
    for (const i of indices) {
      if (checked) next.add(i);
      else next.delete(i);
    }
    selected = next;
  }

  async function browse() {
    const picked = await pickFolder();
    if (picked) folder = picked;
  }

  function confirm() {
    if (!prompt) return;
    const all = !prompt.files || selected.size === prompt.files.length;
    prompt.resolve({
      ...prompt.req,
      save_dir: folder.trim() || null,
      paused,
      files: all ? null : [...selected].sort((a, b) => a - b),
    });
  }
</script>

<Modal open={prompt !== null} title="Add torrent" onclose={() => prompt?.resolve(null)}>
  {#if prompt}
    {#if prompt.title}
      <p class="mb-3 font-medium break-words">{prompt.title}</p>
    {/if}
    <label class="label" for="add-folder">Save to</label>
    <div class="flex gap-2">
      <input
        id="add-folder"
        class="input"
        placeholder={app.settings?.torrents.download_dir ?? 'Default download folder'}
        bind:value={folder}
      />
      {#if isTauri()}
        <button type="button" class="btn-outline shrink-0" onclick={browse}><FolderOpen size={15} /> Browse</button>
      {/if}
    </div>
    {#if !isTauri()}
      <p class="hint">A folder on the server, inside its allowed download folders. Leave empty for the default.</p>
    {/if}

    <div class="mt-4">
      {#if prompt.files}
        <div class="mb-1 flex items-baseline justify-between">
          <span class="label mb-0">Files</span>
          <span class="text-xs text-muted">{selected.size} of {prompt.files.length} · {bytes(total)}</span>
        </div>
        <div class="max-h-64 overflow-y-auto rounded-lg border border-line p-1">
          <FileTree nodes={tree} {selected} onchange={toggle} />
        </div>
      {:else}
        <p class="text-muted">
          The file list isn't known yet. Every file is downloaded; you can pick files in the details once the
          torrent's information arrives.
        </p>
      {/if}
    </div>

    <label class="mt-4 flex items-center gap-2">
      <input type="checkbox" class="size-4 accent-[var(--hl-accent)]" bind:checked={paused} />
      Add paused
    </label>
  {/if}
  {#snippet actions()}
    <button class="btn-ghost" onclick={() => prompt?.resolve(null)}>Cancel</button>
    <button class="btn-primary" disabled={prompt?.files != null && selected.size === 0} onclick={confirm}>Add</button>
  {/snippet}
</Modal>
