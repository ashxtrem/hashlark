<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { CircleAlert, CircleCheck, Info, X } from '@lucide/svelte';
  import { toasts } from '$lib/state/toasts.svelte';

  const styles = {
    info: 'border-line',
    success: 'border-ok/40',
    error: 'border-err/50',
  } as const;
</script>

<div class="pointer-events-none fixed right-4 bottom-4 z-50 flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-2" aria-live="polite">
  {#each toasts.items as toast (toast.id)}
    <div
      class="pointer-events-auto flex items-start gap-3 rounded-xl border bg-surface px-4 py-3 text-sm shadow-card {styles[toast.kind]}"
      role={toast.kind === 'error' ? 'alert' : 'status'}
    >
      <span class="mt-0.5 shrink-0">
        {#if toast.kind === 'success'}
          <CircleCheck size={16} class="text-ok" />
        {:else if toast.kind === 'error'}
          <CircleAlert size={16} class="text-err" />
        {:else}
          <Info size={16} class="text-accent" />
        {/if}
      </span>
      <p class="min-w-0 flex-1 break-words">{toast.message}</p>
      <button class="icon-btn -my-1 -mr-2 size-6" aria-label="Dismiss" onclick={() => toasts.dismiss(toast.id)}>
        <X size={14} />
      </button>
    </div>
  {/each}
</div>
