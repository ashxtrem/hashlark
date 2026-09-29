<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { Copy, KeyRound, Trash2 } from '@lucide/svelte';
  import type { Schemas } from '$lib/api/client';
  import { dateTime } from '$lib/format';
  import { copyText } from '$lib/platform';
  import { app } from '$lib/state/app.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';

  let keys = $state<Schemas['ApiKeyInfo'][]>([]);
  let name = $state('');
  let created = $state<string | null>(null);

  onMount(load);

  async function load() {
    try {
      keys = await app.client.apiKeys();
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    try {
      const key = await app.client.createApiKey(name.trim());
      created = key.key;
      name = '';
      await load();
    } catch (err) {
      toasts.error(errorMessage(err));
    }
  }

  async function revoke(id: string, keyName: string) {
    if (!confirm(`Revoke the key “${keyName}”? Anything using it will be signed out.`)) return;
    try {
      await app.client.revokeApiKey(id);
      await load();
      toasts.success('Key revoked');
    } catch (err) {
      toasts.error(errorMessage(err));
    }
  }
</script>

<section class="card space-y-4 p-5">
  <h2 class="flex items-center gap-2 font-semibold"><KeyRound size={17} /> API keys</h2>
  <p class="text-sm text-muted">
    Keys sign in to this server: the web UI, other devices, or Sonarr/Radarr through
    <code>/torznab/api?apikey=…</code>.
  </p>
  {#if created}
    <div class="rounded-lg bg-ok-soft p-3 text-sm" role="status">
      <p class="font-medium text-ok">Copy the new key now — it won't be shown again.</p>
      <div class="mt-2 flex gap-2">
        <code class="flex-1 truncate rounded bg-surface px-2 py-1 font-mono text-xs">{created}</code>
        <button class="btn-outline" type="button" onclick={async () => { await copyText(created!); toasts.success('Copied'); }}><Copy size={14} /> Copy</button>
      </div>
    </div>
  {/if}
  <form class="flex gap-2" onsubmit={create}>
    <input class="input" placeholder="Name, e.g. Phone or Sonarr" bind:value={name} required maxlength="64" />
    <button class="btn-primary shrink-0" type="submit" disabled={!name.trim()}>Create key</button>
  </form>
  <ul class="divide-y divide-line text-sm">
    {#each keys as k (k.id)}
      <li class="flex items-center gap-3 py-2">
        <span class="flex-1 font-medium">{k.name}</span>
        <span class="text-xs text-muted">{k.last_used_at ? `used ${dateTime(k.last_used_at)}` : 'never used'}</span>
        <button class="icon-btn text-err" type="button" aria-label="Revoke {k.name}" onclick={() => revoke(k.id, k.name)}><Trash2 size={15} /></button>
      </li>
    {/each}
  </ul>
</section>
