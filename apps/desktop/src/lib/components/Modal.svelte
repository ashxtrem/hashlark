<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { X } from '@lucide/svelte';

  interface Props {
    open: boolean;
    title: string;
    onclose?: () => void;
    /** Hide the close button (for dialogs that require a choice). */
    required?: boolean;
    children: Snippet;
    actions?: Snippet;
  }

  let { open, title, onclose, required = false, children, actions }: Props = $props();
  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>

<dialog
  bind:this={dialog}
  class="m-auto w-[min(34rem,calc(100vw-2rem))] rounded-2xl border border-line bg-surface p-0 text-fg shadow-card backdrop:bg-black/40"
  aria-labelledby="modal-title"
  oncancel={(e) => {
    e.preventDefault();
    if (!required) onclose?.();
  }}
>
  <div class="flex items-start justify-between gap-4 px-6 pt-5">
    <h2 id="modal-title" class="text-lg font-semibold">{title}</h2>
    {#if !required}
      <button class="icon-btn -mr-2" aria-label="Close" onclick={() => onclose?.()}>
        <X size={18} />
      </button>
    {/if}
  </div>
  <div class="px-6 py-4 text-sm leading-relaxed">
    {@render children()}
  </div>
  {#if actions}
    <div class="flex justify-end gap-2 border-t border-line px-6 py-4">
      {@render actions()}
    </div>
  {/if}
</dialog>
