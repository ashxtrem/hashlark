<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { CircleAlert, CircleCheck, FileUp, LoaderCircle, Search } from '@lucide/svelte';
  import Modal from './Modal.svelte';
  import { ApiError, type DefinitionCheck, type SearchResult } from '$lib/api/client';
  import { bytes, count } from '$lib/format';
  import { app } from '$lib/state/app.svelte';
  import { providers } from '$lib/state/providers.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  let mode = $state<'definition' | 'torznab'>('definition');
  let tzName = $state('');
  let tzUrl = $state('');
  let tzKey = $state('');

  let yaml = $state('');
  let check = $state<DefinitionCheck | null>(null);
  let checking = $state(false);
  let adding = $state(false);
  let previewQuery = $state('');
  let previewing = $state(false);
  let preview = $state<SearchResult[] | null>(null);
  let previewError = $state<string | null>(null);
  let fileInput: HTMLInputElement | undefined = $state();

  let timer: ReturnType<typeof setTimeout> | undefined;

  // Validate as the user types (debounced).
  $effect(() => {
    const text = yaml;
    clearTimeout(timer);
    check = null;
    preview = null;
    if (!text.trim()) return;
    timer = setTimeout(async () => {
      checking = true;
      try {
        check = await app.client.checkDefinition(text);
      } catch (e) {
        check = { ok: false, errors: [errorMessage(e)], id: null, name: null, version: null };
      } finally {
        checking = false;
      }
    }, 400);
  });

  async function loadFile(e: Event) {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (file) yaml = await file.text();
  }

  async function runPreview() {
    previewing = true;
    previewError = null;
    preview = null;
    try {
      preview = await app.client.previewDefinition(yaml, previewQuery.trim() || 'linux');
    } catch (e) {
      previewError = errorMessage(e);
    } finally {
      previewing = false;
    }
  }

  async function add() {
    adding = true;
    try {
      const view = await app.client.addDefinition(yaml);
      providers.put(view);
      toasts.success(`${view.name} added`);
      reset();
      onclose();
    } catch (e) {
      if (e instanceof ApiError && e.details.length) {
        check = { ok: false, errors: e.details, id: null, name: null, version: null };
      } else {
        toasts.error(errorMessage(e));
      }
    } finally {
      adding = false;
    }
  }

  async function addTorznab(e?: SubmitEvent) {
    e?.preventDefault();
    adding = true;
    try {
      const view = await app.client.addTorznab(tzName.trim(), tzUrl.trim(), tzKey.trim() || null);
      providers.put(view);
      const report = await app.client.testProvider(view.id);
      if (report.ok) toasts.success(`${view.name} added and working (${report.result_count} results for “ubuntu”)`);
      else toasts.error(`${view.name} added, but the test failed: ${report.message ?? report.error_kind}`);
      await providers.load();
      reset();
      onclose();
    } catch (err) {
      toasts.error(errorMessage(err));
    } finally {
      adding = false;
    }
  }

  function reset() {
    tzName = '';
    tzUrl = '';
    tzKey = '';
    yaml = '';
    check = null;
    preview = null;
    previewError = null;
    previewQuery = '';
  }
</script>

<Modal {open} title="Add a provider" onclose={() => { reset(); onclose(); }}>
  <div role="tablist" class="mb-4 flex gap-1 rounded-lg bg-surface-2 p-1">
    <button role="tab" aria-selected={mode === 'definition'} class="flex-1 rounded-md px-3 py-1.5 text-sm font-medium {mode === 'definition' ? 'bg-surface shadow-card' : 'text-muted'}" onclick={() => (mode = 'definition')}>Definition file</button>
    <button role="tab" aria-selected={mode === 'torznab'} class="flex-1 rounded-md px-3 py-1.5 text-sm font-medium {mode === 'torznab' ? 'bg-surface shadow-card' : 'text-muted'}" onclick={() => (mode = 'torznab')}>Jackett / Prowlarr</button>
  </div>
  {#if mode === 'torznab'}
    <form id="torznab-form" class="space-y-3" onsubmit={addTorznab}>
      <p class="text-muted">
        Connect a Torznab endpoint to search every indexer configured there, including private
        trackers.
      </p>
      <label class="block">
        <span class="label">Name</span>
        <input class="input" placeholder="My Jackett" bind:value={tzName} required />
      </label>
      <label class="block">
        <span class="label">Torznab URL</span>
        <input class="input" placeholder="http://127.0.0.1:9117/api/v2.0/indexers/all/results/torznab/api" bind:value={tzUrl} required />
        <span class="hint">Jackett: copy “Torznab Feed” of an indexer, or use <code>…/indexers/all/results/torznab/api</code>. Prowlarr: <code>http://host:9696/&lt;indexer id&gt;/api</code>.</span>
      </label>
      <label class="block">
        <span class="label">API key</span>
        <input class="input" type="password" autocomplete="off" bind:value={tzKey} />
        <span class="hint">Stored in your system keychain.</span>
      </label>
    </form>
  {:else}
  <div class="space-y-4">
    <p class="text-muted">
      Paste a provider definition (YAML) or load a <code>.yml</code> file. Definitions only describe
      a site; they can't run code.
    </p>
    <div>
      <div class="mb-1 flex items-center justify-between">
        <label class="label mb-0" for="definition-yaml">Definition</label>
        <button type="button" class="btn-ghost px-2 py-1 text-xs" onclick={() => fileInput?.click()}>
          <FileUp size={14} /> Load file…
        </button>
        <input bind:this={fileInput} type="file" accept=".yml,.yaml,text/yaml" class="hidden" onchange={loadFile} />
      </div>
      <textarea
        id="definition-yaml"
        class="input h-56 resize-y font-mono text-xs leading-relaxed"
        spellcheck="false"
        placeholder={'schema: 1\nid: my-site\nname: My Site\nlinks: [https://example.org/]\nsearch: …'}
        bind:value={yaml}
      ></textarea>
    </div>

    {#if checking}
      <p class="flex items-center gap-2 text-muted"><LoaderCircle size={14} class="animate-spin" /> Checking…</p>
    {:else if check?.ok}
      <p class="flex items-center gap-2 text-ok"><CircleCheck size={15} /> Valid: {check.name} (<code>{check.id}</code>, version {check.version})</p>
    {:else if check}
      <div class="rounded-lg bg-err-soft px-3 py-2 text-err" role="alert">
        <p class="flex items-center gap-2 font-medium"><CircleAlert size={15} /> {check.errors.length} problem{check.errors.length === 1 ? '' : 's'}</p>
        <ul class="mt-1 list-disc space-y-0.5 pl-6 text-xs">
          {#each check.errors as error, i (i)}<li>{error}</li>{/each}
        </ul>
      </div>
    {/if}

    {#if check?.ok}
      <form class="flex gap-2" onsubmit={(e) => { e.preventDefault(); runPreview(); }}>
        <input class="input" placeholder="Try a search, e.g. ubuntu" bind:value={previewQuery} />
        <button class="btn-outline shrink-0" type="submit" disabled={previewing}>
          {#if previewing}<LoaderCircle size={15} class="animate-spin" />{:else}<Search size={15} />{/if}
          Preview
        </button>
      </form>
      {#if previewError}
        <p class="rounded-lg bg-err-soft px-3 py-2 text-err" role="alert">{previewError}</p>
      {:else if preview}
        <div class="max-h-48 overflow-y-auto rounded-lg border border-line">
          {#if preview.length === 0}
            <p class="p-3 text-muted">No results for that search.</p>
          {:else}
            <table class="w-full table-fixed text-xs">
              <colgroup><col /><col class="w-20" /><col class="w-16" /></colgroup>
              <tbody>
                {#each preview.slice(0, 20) as r, i (i)}
                  <tr class="border-b border-line last:border-0">
                    <td class="truncate px-3 py-1.5" title={r.title}>{r.title}</td>
                    <td class="px-2 text-right whitespace-nowrap text-muted">{bytes(r.size_bytes)}</td>
                    <td class="px-3 text-right whitespace-nowrap text-muted">{count(r.seeders)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
            <p class="border-t border-line px-3 py-1.5 text-muted">{preview.length} results</p>
          {/if}
        </div>
      {/if}
    {/if}
  </div>
  {/if}
  {#snippet actions()}
    <button class="btn-ghost" onclick={() => { reset(); onclose(); }}>Cancel</button>
    {#if mode === 'torznab'}
      <button class="btn-primary" type="submit" form="torznab-form" disabled={adding || !tzName.trim() || !tzUrl.trim()}>
        {#if adding}<LoaderCircle size={15} class="animate-spin" />{/if} Add and test
      </button>
    {:else}
      <button class="btn-primary" disabled={!check?.ok || adding} onclick={add}>Add provider</button>
    {/if}
  {/snippet}
</Modal>
