<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { ChevronDown, Copy, FlaskConical, LoaderCircle, Plus, Trash2 } from '@lucide/svelte';
  import type { ProviderView, Schemas } from '$lib/api/client';
  import AddProviderDialog from '$components/AddProviderDialog.svelte';
  import HealthBadge from '$components/HealthBadge.svelte';
  import Modal from '$components/Modal.svelte';
  import ProviderSettingsForm from '$components/ProviderSettingsForm.svelte';
  import RepositoriesPanel from '$components/RepositoriesPanel.svelte';
  import Switch from '$components/Switch.svelte';
  import { CATEGORY_LABELS, duration, ERROR_LABELS } from '$lib/format';
  import { startChallenge } from '$lib/actions';
  import { copyText } from '$lib/platform';
  import { app } from '$lib/state/app.svelte';
  import { providers } from '$lib/state/providers.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';

  type Route = Schemas['Route'];

  const KIND_LABELS = { native: 'Built-in', definition: 'Definition', torznab: 'Torznab' } as const;
  const ROUTES: { value: Route; label: string }[] = [
    { value: 'default', label: 'Use global network settings' },
    { value: 'direct', label: 'Direct (no proxy or Tor)' },
    { value: 'proxy', label: 'Through its own proxy' },
    { value: 'tor', label: 'Always through Tor' },
  ];

  let testing = $state<Record<string, boolean>>({});
  let expanded = $state<Record<string, boolean>>({});
  let drafts = $state<Record<string, { route: Route; proxy: string }>>({});
  let adding = $state(false);
  let viewing = $state<{ name: string; yaml: string } | null>(null);

  let enabledCount = $derived(providers.list.filter((p) => p.enabled).length);

  onMount(() => providers.load());

  async function patch(p: ProviderView, body: Schemas['ProviderPatch'], message?: string) {
    try {
      providers.put(await app.client.updateProvider(p.id, body));
      if (message) toasts.success(message);
    } catch (e) {
      toasts.error(errorMessage(e));
      await providers.load();
    }
  }

  async function test(p: ProviderView) {
    testing[p.id] = true;
    try {
      const report = await app.client.testProvider(p.id);
      if (report.ok) {
        toasts.success(`${p.name} works: ${report.result_count} results in ${duration(report.latency_ms)}`);
      } else {
        const kind = report.error_kind ? ERROR_LABELS[report.error_kind] : 'Failed';
        toasts.error(`${p.name}: ${kind}. ${report.message ?? ''}`);
      }
      await providers.load();
    } catch (e) {
      toasts.error(errorMessage(e));
    } finally {
      testing[p.id] = false;
    }
  }

  async function remove(p: ProviderView) {
    if (!confirm(`Remove ${p.name}? Its settings and history will be deleted.`)) return;
    try {
      await app.client.deleteProvider(p.id);
      providers.remove(p.id);
      toasts.success(`${p.name} removed`);
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  function toggleExpanded(p: ProviderView) {
    expanded[p.id] = !expanded[p.id];
    if (expanded[p.id]) {
      drafts[p.id] = { route: p.network_policy.route, proxy: p.network_policy.proxy ?? '' };
    }
  }

  async function saveNetwork(p: ProviderView) {
    const d = drafts[p.id];
    if (!d) return;
    await patch(
      p,
      { network_policy: { route: d.route, proxy: d.route === 'proxy' ? d.proxy.trim() || null : null } },
      'Network settings saved',
    );
  }

  async function viewDefinition(p: ProviderView) {
    try {
      const stored = await app.client.definition(p.id);
      viewing = { name: p.name, yaml: stored.yaml };
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  function sourceLabel(source: ProviderView['source']): string {
    if (source.type === 'definition') {
      const from = source.repo_id === 'builtin' ? 'built in' : source.repo_id ? `from ${source.repo_id}` : 'imported';
      return `Definition v${source.version}, ${from}`;
    }
    if (source.type === 'torznab') return `Torznab: ${source.url}`;
    return 'Compiled into Hashlark';
  }
</script>

<svelte:head><title>Providers – Hashlark</title></svelte:head>

<div class="mx-auto max-w-4xl px-6 py-6">
  <header class="mb-5 flex flex-wrap items-end justify-between gap-3">
    <div>
      <h1 class="text-2xl font-semibold">Providers</h1>
      <p class="mt-1 text-sm text-muted">
        {enabledCount} of {providers.list.length} enabled. Providers that keep failing are paused
        automatically and retried later.
      </p>
    </div>
    <div class="flex gap-2">
      <a class="btn-outline" href="/editor">Write a definition</a>
      <button class="btn-primary" onclick={() => (adding = true)}><Plus size={16} /> Add provider</button>
    </div>
  </header>

  {#if !providers.loaded}
    <p class="flex items-center gap-2 text-muted"><LoaderCircle size={16} class="animate-spin" /> Loading…</p>
  {:else if providers.error}
    <p class="text-err" role="alert">{providers.error}</p>
  {:else}
    <ul class="space-y-3">
      {#each providers.list as p (p.id)}
        {@const needsSetup = p.settings.some((s) => s.required && !s.is_set && !s.value)}
        <li class="card p-4">
          <div class="flex items-start gap-4">
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-2">
                <h2 class="font-semibold">{p.name}</h2>
                <span class="rounded-md bg-surface-2 px-1.5 py-0.5 text-xs text-muted">{KIND_LABELS[p.kind]}</span>
                {#if p.source.type === 'definition' && p.source.site_type && p.source.site_type !== 'public'}
                  <span class="rounded-md bg-warn-soft px-1.5 py-0.5 text-xs text-warn">{p.source.site_type === 'private' ? 'Private' : 'Semi-private'}</span>
                {/if}
              </div>
              {#if p.description}<p class="mt-1 text-sm text-muted">{p.description}</p>{/if}
              {#if p.error}
                <p class="mt-2 rounded-lg bg-err-soft px-3 py-2 text-sm text-err" role="alert">Can't load: {p.error}</p>
              {:else if p.health.last_error_kind === 'challenge_required'}
                <p class="mt-2 flex flex-wrap items-center gap-2 rounded-lg bg-warn-soft px-3 py-2 text-sm text-warn">
                  The site asks for a browser check.
                  <button class="underline" onclick={() => startChallenge(p.id)}>Open site to continue</button>
                </p>
              {:else if p.health.last_error_kind === 'blocked'}
                <p class="mt-2 rounded-lg bg-warn-soft px-3 py-2 text-sm text-warn">
                  The site seems blocked on this network. Try encrypted DNS, a proxy or Tor in
                  <a class="underline" href="/settings">Settings</a>, or set a route under Details.
                </p>
              {:else if needsSetup}
                <p class="mt-2 rounded-lg bg-warn-soft px-3 py-2 text-sm text-warn">Needs settings before it can search. Open “Details”.</p>
              {/if}
              <div class="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1">
                <HealthBadge health={p.health} />
                <span class="text-xs text-muted">
                  {p.categories.length ? p.categories.map((c) => CATEGORY_LABELS[c]).join(', ') : 'All categories'}
                </span>
              </div>
            </div>
            <Switch
              checked={p.enabled}
              label="{p.enabled ? 'Disable' : 'Enable'} {p.name}"
              onchange={(on) => patch(p, { enabled: on }, `${p.name} ${on ? 'enabled' : 'disabled'}`)}
            />
          </div>

          <div class="mt-3 flex flex-wrap gap-2">
            <button class="btn-outline" disabled={testing[p.id] || Boolean(p.error)} onclick={() => test(p)}>
              {#if testing[p.id]}<LoaderCircle size={15} class="animate-spin" />{:else}<FlaskConical size={15} />{/if}
              Test
            </button>
            <button class="btn-ghost" aria-expanded={expanded[p.id] ?? false} onclick={() => toggleExpanded(p)}>
              Details <ChevronDown size={15} class="transition-transform {expanded[p.id] ? 'rotate-180' : ''}" />
            </button>
            {#if !p.builtin}
              <button class="btn-danger ml-auto" onclick={() => remove(p)}><Trash2 size={15} /> Remove</button>
            {/if}
          </div>

          {#if expanded[p.id] && drafts[p.id]}
            {@const d = drafts[p.id]!}
            <div class="mt-3 space-y-4 rounded-lg bg-surface-2 p-4">
              {#if p.settings.length}
                <section>
                  <h3 class="mb-2 text-sm font-semibold">Settings</h3>
                  <ProviderSettingsForm provider={p} />
                </section>
              {/if}
              <section>
                <h3 class="mb-2 text-sm font-semibold">Network</h3>
                <form class="grid gap-3 sm:grid-cols-[1fr_1fr_auto] sm:items-end" onsubmit={(e) => { e.preventDefault(); saveNetwork(p); }}>
                  <label>
                    <span class="label">Route</span>
                    <select class="input" bind:value={d.route}>
                      {#each ROUTES as r (r.value)}<option value={r.value}>{r.label}</option>{/each}
                    </select>
                  </label>
                  <label>
                    <span class="label">Proxy URL</span>
                    <input class="input" placeholder="socks5h://127.0.0.1:9050" disabled={d.route !== 'proxy'} bind:value={d.proxy} />
                  </label>
                  <button class="btn-primary" type="submit">Save</button>
                </form>
              </section>
              <section class="text-sm">
                <h3 class="mb-1 font-semibold">Source</h3>
                <p class="text-muted">{sourceLabel(p.source)}</p>
                {#if p.source.type === 'definition'}
                  {#if p.source.links.length}
                    <p class="mt-1 text-xs text-muted">Mirrors: {p.source.links.join(' · ')}</p>
                  {/if}
                  <button class="btn-ghost mt-2 -ml-2 px-2 py-1 text-xs" onclick={() => viewDefinition(p)}>View definition</button>
                {/if}
              </section>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<div class="mx-auto max-w-4xl px-6 pb-10">
  <RepositoriesPanel />
</div>

<AddProviderDialog open={adding} onclose={() => (adding = false)} />

<Modal open={viewing !== null} title={viewing ? `${viewing.name} definition` : ''} onclose={() => (viewing = null)}>
  <pre class="max-h-96 overflow-auto rounded-lg bg-surface-2 p-3 font-mono text-xs">{viewing?.yaml}</pre>
  {#snippet actions()}
    <button class="btn-outline" onclick={async () => { if (viewing) { await copyText(viewing.yaml); toasts.success('Copied'); } }}><Copy size={15} /> Copy</button>
    <button class="btn-primary" onclick={() => (viewing = null)}>Close</button>
  {/snippet}
</Modal>
