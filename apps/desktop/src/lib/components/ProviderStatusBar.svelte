<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { CircleCheck, CircleX, LoaderCircle } from '@lucide/svelte';
  import { startChallenge } from '$lib/actions';
  import { duration, ERROR_LABELS } from '$lib/format';
  import type { ProviderRun } from '$lib/state/search.svelte';

  interface Props {
    runs: ProviderRun[];
    names: Record<string, string>;
  }

  let { runs, names }: Props = $props();

  function tooltip(run: ProviderRun): string {
    if (run.state === 'running') return 'Searching…';
    if (run.state === 'ok') return `${run.count} results in ${duration(run.latencyMs)}`;
    return run.message ?? 'Failed';
  }
</script>

{#if runs.length > 0}
  <ul class="flex flex-wrap gap-2" aria-label="Provider status">
    {#each runs as run (run.id)}
      <li
        class="chip {run.state === 'failed' ? 'border-err/40 bg-err-soft' : ''}"
        title={tooltip(run)}
      >
        {#if run.errorKind === 'challenge_required'}
          <button class="contents" onclick={() => startChallenge(run.id)} aria-label="Open {names[run.id] ?? run.id} to pass its browser check">
            <CircleX size={13} class="text-err" />
            <span>{names[run.id] ?? run.id}</span>
            <span class="text-err underline">Open site to continue</span>
          </button>
        {:else}
        {#if run.state === 'running'}
          <LoaderCircle size={13} class="animate-spin text-muted" />
        {:else if run.state === 'ok'}
          <CircleCheck size={13} class="text-ok" />
        {:else}
          <CircleX size={13} class="text-err" />
        {/if}
        <span>{names[run.id] ?? run.id}</span>
        {#if run.state === 'ok'}
          <span class="text-muted">{run.count} · {duration(run.latencyMs)}</span>
        {:else if run.state === 'failed'}
          <span class="text-err">{ERROR_LABELS[run.errorKind ?? ''] ?? 'Failed'}</span>
        {/if}
        {/if}
      </li>
    {/each}
  </ul>
{/if}
