<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { LoaderCircle, Plus, RefreshCw, ShieldCheck, Trash2 } from '@lucide/svelte';
  import type { RepoView, SyncReport } from '$lib/api/client';
  import { dateTime } from '$lib/format';
  import { app } from '$lib/state/app.svelte';
  import { providers } from '$lib/state/providers.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';

  let repos = $state<RepoView[] | null>(null);
  let url = $state('');
  let adding = $state(false);
  let syncing = $state<Record<string, boolean>>({});

  onMount(load);

  async function load() {
    try {
      repos = await app.client.repos();
    } catch (e) {
      toasts.error(errorMessage(e));
      repos = [];
    }
  }

  function summary(r: SyncReport): string {
    const parts = [];
    if (r.added.length) parts.push(`${r.added.length} new (disabled until you enable them)`);
    if (r.updated.length) parts.push(`${r.updated.length} updated`);
    if (r.removed.length) parts.push(`${r.removed.length} removed`);
    if (!parts.length) parts.push('no changes');
    if (r.errors.length) parts.push(`${r.errors.length} skipped`);
    return parts.join(', ');
  }

  async function add(e: SubmitEvent) {
    e.preventDefault();
    adding = true;
    try {
      const { repo, sync } = await app.client.addRepo(url.trim());
      toasts.success(`Added ${repo.name ?? 'repository'}: ${summary(sync)}`);
      for (const err of sync.errors) toasts.error(err);
      url = '';
      await Promise.all([load(), providers.load()]);
    } catch (err) {
      toasts.error(errorMessage(err));
    } finally {
      adding = false;
    }
  }

  async function sync(r: RepoView) {
    syncing[r.id] = true;
    try {
      const report = await app.client.syncRepo(r.id);
      toasts.success(`${r.name ?? r.id}: ${summary(report)}`);
      await Promise.all([load(), providers.load()]);
    } catch (err) {
      toasts.error(errorMessage(err));
    } finally {
      syncing[r.id] = false;
    }
  }

  async function remove(r: RepoView) {
    if (!confirm(`Remove ${r.name ?? r.url} and its ${r.definitions} provider(s)?`)) return;
    try {
      await app.client.removeRepo(r.id);
      toasts.success('Repository removed');
      await Promise.all([load(), providers.load()]);
    } catch (err) {
      toasts.error(errorMessage(err));
    }
  }
</script>

<section class="mt-10">
  <h2 class="text-lg font-semibold">Definition repositories</h2>
  <p class="mt-1 text-sm text-muted">
    Signed collections of provider definitions, updated automatically once a day. The signing key
    seen when you add a repository is remembered; updates signed with another key are refused.
  </p>

  <form class="mt-3 flex gap-2" onsubmit={add}>
    <input class="input" type="url" placeholder="https://example.org/hashlark-definitions/" bind:value={url} required />
    <button class="btn-primary shrink-0" type="submit" disabled={adding || !url.trim()}>
      {#if adding}<LoaderCircle size={15} class="animate-spin" />{:else}<Plus size={15} />{/if} Add
    </button>
  </form>

  {#if repos === null}
    <p class="mt-3 flex items-center gap-2 text-muted"><LoaderCircle size={16} class="animate-spin" /> Loading…</p>
  {:else}
    <ul class="mt-3 space-y-2">
      {#each repos as r (r.id)}
        <li class="card flex flex-wrap items-center gap-3 p-3 text-sm">
          <div class="min-w-0 flex-1">
            <p class="font-medium">{r.name ?? r.url}</p>
            <p class="truncate text-xs text-muted">
              {#if r.builtin}Shipped with Hashlark{:else}{r.url}{/if} · {r.definitions} definition{r.definitions === 1 ? '' : 's'}
              {#if r.last_sync_at} · synced {dateTime(r.last_sync_at)}{/if}
            </p>
            {#if r.fingerprint}
              <p class="mt-0.5 flex items-center gap-1 text-xs text-muted"><ShieldCheck size={12} /> Key {r.fingerprint}</p>
            {/if}
          </div>
          {#if !r.builtin}
            <button class="btn-outline" disabled={syncing[r.id]} onclick={() => sync(r)}>
              <RefreshCw size={14} class={syncing[r.id] ? 'animate-spin' : ''} /> Sync
            </button>
            <button class="btn-danger" onclick={() => remove(r)} aria-label="Remove {r.name ?? r.url}"><Trash2 size={14} /></button>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>
