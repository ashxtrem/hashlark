<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { Trash2 } from '@lucide/svelte';
  import Modal from './Modal.svelte';

  interface Props {
    /** Torrents to delete; the dialog is open while this is non-empty. */
    targets: { id: string; name: string }[];
    onclose: () => void;
    /** Called with the ids and whether to delete the downloaded files. */
    onconfirm: (ids: string[], deleteFiles: boolean) => void;
  }

  let { targets, onclose, onconfirm }: Props = $props();
  let deleteFiles = $state(false);

  // Unticked every time the dialog opens.
  $effect(() => {
    if (targets.length > 0) deleteFiles = false;
  });

  const SHOWN = 8;
</script>

<Modal
  open={targets.length > 0}
  title={targets.length === 1 ? 'Delete this torrent?' : `Delete ${targets.length} torrents?`}
  {onclose}
>
  <ul class="max-h-48 space-y-1 overflow-y-auto rounded-lg bg-surface-2 px-3 py-2" aria-label="Torrents to delete">
    {#each targets.slice(0, SHOWN) as t (t.id)}
      <li class="truncate" title={t.name}>{t.name}</li>
    {/each}
    {#if targets.length > SHOWN}
      <li class="text-muted">and {targets.length - SHOWN} more</li>
    {/if}
  </ul>
  <p class="mt-3 text-muted">The torrent and its stored state are removed from Hashlark.</p>
  <label class="mt-3 flex items-center gap-2">
    <input type="checkbox" class="size-4 accent-[var(--hl-err)]" bind:checked={deleteFiles} />
    Also delete downloaded files
  </label>
  {#snippet actions()}
    <button class="btn-ghost" onclick={onclose}>Cancel</button>
    <button
      class="btn bg-err text-white hover:opacity-90"
      onclick={() => onconfirm(targets.map((t) => t.id), deleteFiles)}
    >
      <Trash2 size={15} />
      {deleteFiles ? 'Delete and remove files' : 'Delete'}
    </button>
  {/snippet}
</Modal>
