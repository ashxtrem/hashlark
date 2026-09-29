<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { CircleAlert, CircleCheck, FileInput, LoaderCircle, MousePointerClick, Play, Save } from '@lucide/svelte';
  import type { DefinitionCheck, SearchResult } from '$lib/api/client';
  import Modal from '$components/Modal.svelte';
  import { applyPick, applyUrl, currentRows, parses, PICK_LABELS, type PickTarget } from '$lib/editor';
  import { bytes, count } from '$lib/format';
  import { relativeSelector, rowSelector, sanitizePage } from '$lib/picker';
  import { app } from '$lib/state/app.svelte';
  import { providers } from '$lib/state/providers.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';

  const DRAFT_KEY = 'hashlark.editor.draft';

  let yaml = $state('');
  let check = $state<DefinitionCheck | null>(null);
  let tab = $state<'pick' | 'preview'>('pick');

  // Picker
  let pageUrl = $state('');
  let term = $state('ubuntu');
  let loading = $state(false);
  let srcdoc = $state<string | null>(null);
  let frame: HTMLIFrameElement | undefined = $state();
  let target = $state<PickTarget>('rows');
  let rowMatches = $state<number | null>(null);

  // Preview
  let previewQuery = $state('ubuntu');
  let previewing = $state(false);
  let results = $state<SearchResult[] | null>(null);
  let previewError = $state<string | null>(null);

  // Jackett import
  let importing = $state(false);
  let jackettYaml = $state('');
  let importNotes = $state<string[]>([]);

  let saving = $state(false);

  onMount(async () => {
    let draft: string | null = null;
    try {
      draft = localStorage.getItem(DRAFT_KEY);
    } catch {
      // Storage unavailable: start fresh.
    }
    yaml = draft ?? (await app.client.starterDefinition('html', 'my-site'));
  });

  // Remember the draft, and re-check it as it changes.
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const text = yaml;
    if (!text) return;
    try {
      localStorage.setItem(DRAFT_KEY, text);
    } catch {
      // Not critical.
    }
    clearTimeout(timer);
    timer = setTimeout(async () => {
      try {
        check = await app.client.checkDefinition(text);
      } catch (e) {
        check = { ok: false, errors: [errorMessage(e)], id: null, name: null, version: null };
      }
      highlightRows();
    }, 400);
  });

  async function startFrom(kind: 'html' | 'json' | 'rss') {
    if (yaml && !confirm('Replace the current definition with a new starter?')) return;
    yaml = await app.client.starterDefinition(kind, 'my-site');
    srcdoc = null;
  }

  async function loadPage(e: SubmitEvent) {
    e.preventDefault();
    loading = true;
    try {
      const page = await app.client.fetchPage(pageUrl.trim());
      const doc = sanitizePage(page.html);
      const style = doc.createElement('style');
      style.textContent =
        '.hl-hover{outline:2px dashed #8b82ff!important;cursor:crosshair}' +
        '.hl-row{outline:1px solid #13855b!important;background:rgba(19,133,91,.06)!important}' +
        'body{font:13px system-ui,sans-serif;background:#fff;color:#111}';
      doc.head.append(style);
      srcdoc = '<!doctype html>' + doc.documentElement.outerHTML;
      yaml = applyUrl(yaml, page.url, term);
      target = 'rows';
    } catch (err) {
      toasts.error(errorMessage(err));
    } finally {
      loading = false;
    }
  }

  function frameDoc(): Document | null {
    return frame?.contentDocument ?? null;
  }

  function highlightRows() {
    const doc = frameDoc();
    if (!doc) return;
    doc.querySelectorAll('.hl-row').forEach((e) => e.classList.remove('hl-row'));
    const rows = currentRows(yaml);
    if (!rows) return;
    try {
      const found = doc.querySelectorAll(rows);
      found.forEach((e) => e.classList.add('hl-row'));
      rowMatches = found.length;
    } catch {
      rowMatches = null;
    }
  }

  function onFrameLoad() {
    const doc = frameDoc();
    if (!doc) return;
    doc.addEventListener('mouseover', (e) => (e.target as Element).classList?.add('hl-hover'));
    doc.addEventListener('mouseout', (e) => (e.target as Element).classList?.remove('hl-hover'));
    doc.addEventListener(
      'click',
      (e) => {
        e.preventDefault();
        e.stopPropagation();
        pick(e.target as Element);
      },
      true,
    );
    highlightRows();
  }

  function pick(el: Element) {
    el.classList.remove('hl-hover');
    if (target === 'rows') {
      const found = rowSelector(el);
      if (!found) {
        toasts.error('No repeating rows around that element; click inside one result.');
        return;
      }
      yaml = applyPick(yaml, 'rows', found.selector);
      target = 'title';
      return;
    }
    const rows = currentRows(yaml);
    const row = rows ? el.closest(rows) : null;
    if (!row) {
      toasts.error('Pick the result rows first, then click inside a row.');
      return;
    }
    // For links, use the link element itself.
    const node = ['details', 'magnet', 'download'].includes(target) ? (el.closest('a') ?? el) : el;
    const selector = relativeSelector(row, node);
    if (!selector) {
      toasts.error('Could not build a selector for that element; try a nearby one.');
      return;
    }
    yaml = applyPick(yaml, target, selector);
    const order: PickTarget[] = ['title', 'details', 'magnet', 'size', 'seeders', 'leechers', 'date'];
    const next = order[order.indexOf(target) + 1];
    if (next) target = next;
  }

  async function runPreview(e?: SubmitEvent) {
    e?.preventDefault();
    tab = 'preview';
    previewing = true;
    previewError = null;
    results = null;
    try {
      results = await app.client.previewDefinition(yaml, previewQuery.trim() || 'ubuntu');
    } catch (err) {
      previewError = errorMessage(err);
    } finally {
      previewing = false;
    }
  }

  async function save() {
    saving = true;
    try {
      const view = await app.client.addDefinition(yaml);
      providers.put(view);
      toasts.success(`${view.name} saved as a provider`);
    } catch (err) {
      toasts.error(errorMessage(err));
    } finally {
      saving = false;
    }
  }

  async function convertJackett() {
    try {
      const result = await app.client.convertCardigann(jackettYaml);
      yaml = result.yaml;
      importNotes = result.warnings;
      importing = false;
      jackettYaml = '';
      toasts.success(result.valid ? 'Converted. Review the notes, then preview.' : 'Converted with problems; see the checks.');
    } catch (err) {
      toasts.error(errorMessage(err));
    }
  }

  const TARGETS = Object.keys(PICK_LABELS) as PickTarget[];
</script>

<svelte:head><title>Definition editor – Hashlark</title></svelte:head>

<div class="flex h-full flex-col">
  <header class="flex flex-wrap items-center gap-2 border-b border-line px-5 py-3">
    <h1 class="mr-3 text-lg font-semibold">Definition editor</h1>
    <span class="text-sm text-muted">New:</span>
    <button class="btn-ghost px-2 py-1 text-sm" onclick={() => startFrom('html')}>HTML</button>
    <button class="btn-ghost px-2 py-1 text-sm" onclick={() => startFrom('json')}>JSON</button>
    <button class="btn-ghost px-2 py-1 text-sm" onclick={() => startFrom('rss')}>RSS</button>
    <button class="btn-ghost px-2 py-1 text-sm" onclick={() => (importing = true)}><FileInput size={14} /> From Jackett</button>
    <span class="ml-auto flex items-center gap-2 text-sm">
      {#if !yaml}
        <LoaderCircle size={14} class="animate-spin" />
      {:else if !parses(yaml)}
        <span class="text-err">YAML syntax error</span>
      {:else if check?.ok}
        <span class="flex items-center gap-1 text-ok"><CircleCheck size={14} /> Valid</span>
      {:else if check}
        <span class="flex items-center gap-1 text-err"><CircleAlert size={14} /> {check.errors.length} problem{check.errors.length === 1 ? '' : 's'}</span>
      {/if}
    </span>
    <button class="btn-primary" disabled={!check?.ok || saving} onclick={save}><Save size={15} /> Save as provider</button>
  </header>

  <div class="grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-2">
    <section class="flex min-h-0 flex-col border-r border-line">
      <textarea
        class="min-h-0 flex-1 resize-none border-0 bg-surface p-4 font-mono text-xs leading-relaxed text-fg focus:outline-none"
        spellcheck="false"
        aria-label="Definition YAML"
        bind:value={yaml}
      ></textarea>
      {#if check && !check.ok}
        <ul class="max-h-36 overflow-y-auto border-t border-line bg-err-soft px-4 py-2 text-xs text-err" role="alert">
          {#each check.errors as error, i (i)}<li>• {error}</li>{/each}
        </ul>
      {/if}
      {#if importNotes.length}
        <details class="border-t border-line bg-warn-soft px-4 py-2 text-xs text-warn">
          <summary class="cursor-pointer font-medium">{importNotes.length} conversion note{importNotes.length === 1 ? '' : 's'}</summary>
          <ul class="mt-1 space-y-0.5">{#each importNotes as note, i (i)}<li>• {note}</li>{/each}</ul>
        </details>
      {/if}
    </section>

    <section class="flex min-h-0 flex-col">
      <div role="tablist" class="flex gap-1 border-b border-line px-4 pt-2">
        <button role="tab" aria-selected={tab === 'pick'} class="rounded-t-lg px-3 py-1.5 text-sm {tab === 'pick' ? 'bg-surface-2 font-medium' : 'text-muted'}" onclick={() => (tab = 'pick')}>
          <MousePointerClick size={14} class="mr-1 inline" /> Pick from page
        </button>
        <button role="tab" aria-selected={tab === 'preview'} class="rounded-t-lg px-3 py-1.5 text-sm {tab === 'preview' ? 'bg-surface-2 font-medium' : 'text-muted'}" onclick={() => (tab = 'preview')}>
          <Play size={14} class="mr-1 inline" /> Preview
        </button>
      </div>

      {#if tab === 'pick'}
        <div class="flex min-h-0 flex-1 flex-col gap-3 p-4">
          <form class="grid gap-2 sm:grid-cols-[1fr_8rem_auto]" onsubmit={loadPage}>
            <input class="input" type="url" placeholder="A search results page, e.g. https://site.example/search?q=ubuntu" bind:value={pageUrl} required />
            <input class="input" placeholder="Searched words" bind:value={term} title="The words searched in that URL; they become {'{{ query.text }}'}" />
            <button class="btn-outline" type="submit" disabled={loading}>
              {#if loading}<LoaderCircle size={15} class="animate-spin" />{/if} Load
            </button>
          </form>
          {#if srcdoc}
            <div class="flex flex-wrap items-center gap-1.5 text-xs" role="group" aria-label="What to pick">
              <span class="text-muted">Click to pick:</span>
              {#each TARGETS as t (t)}
                <button class="chip {target === t ? 'border-accent bg-accent-soft text-accent' : 'hover:bg-surface-2'}" aria-pressed={target === t} onclick={() => (target = t)}>{PICK_LABELS[t]}</button>
              {/each}
              {#if rowMatches !== null}<span class="ml-auto text-muted">{rowMatches} rows match</span>{/if}
            </div>
            <iframe
              bind:this={frame}
              title="Page for picking elements"
              class="min-h-0 flex-1 rounded-lg border border-line bg-white"
              sandbox="allow-same-origin"
              {srcdoc}
              onload={onFrameLoad}
            ></iframe>
          {:else}
            <p class="text-sm text-muted">
              Load a search results page from the site, then click a result to pick the rows, and
              click its title, links, size and so on to fill in the fields. Pages are shown without
              scripts or images.
            </p>
          {/if}
        </div>
      {:else}
        <div class="flex min-h-0 flex-1 flex-col gap-3 p-4">
          <form class="flex gap-2" onsubmit={runPreview}>
            <input class="input" placeholder="Search words" bind:value={previewQuery} />
            <button class="btn-primary shrink-0" type="submit" disabled={previewing || !check?.ok}>
              {#if previewing}<LoaderCircle size={15} class="animate-spin" />{:else}<Play size={15} />{/if} Run
            </button>
          </form>
          {#if previewError}
            <p class="rounded-lg bg-err-soft px-3 py-2 text-sm text-err" role="alert">{previewError}</p>
          {:else if results}
            <p class="text-sm text-muted">{results.length} results</p>
            <div class="min-h-0 flex-1 overflow-y-auto rounded-lg border border-line">
              <table class="w-full table-fixed text-xs">
                <colgroup><col /><col class="w-20" /><col class="w-16" /><col class="w-20" /></colgroup>
                <thead class="bg-surface-2 text-left text-muted">
                  <tr><th class="px-3 py-1.5">Title</th><th class="px-2 text-right">Size</th><th class="px-2 text-right">Seeds</th><th class="px-3">Link</th></tr>
                </thead>
                <tbody>
                  {#each results as r, i (i)}
                    <tr class="border-t border-line">
                      <td class="truncate px-3 py-1.5" title={r.title}>{r.title}</td>
                      <td class="px-2 text-right text-muted">{bytes(r.size_bytes)}</td>
                      <td class="px-2 text-right text-muted">{count(r.seeders)}</td>
                      <td class="px-3 text-muted">{r.magnet ? 'magnet' : r.info_hash ? 'hash' : r.torrent_url ? '.torrent' : r.needs_resolve ? 'details' : '–'}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {:else}
            <p class="text-sm text-muted">Runs the definition against the live site without saving it.</p>
          {/if}
        </div>
      {/if}
    </section>
  </div>
</div>

<Modal open={importing} title="Convert a Jackett definition" onclose={() => (importing = false)}>
  <p class="text-muted">
    Paste a Jackett (Cardigann) <code>.yml</code>. Hashlark converts what it can and lists anything
    that needs review. The conversion happens on this device only.
  </p>
  <textarea class="input mt-3 h-56 font-mono text-xs" spellcheck="false" bind:value={jackettYaml} aria-label="Jackett definition"></textarea>
  {#snippet actions()}
    <button class="btn-ghost" onclick={() => (importing = false)}>Cancel</button>
    <button class="btn-primary" disabled={!jackettYaml.trim()} onclick={convertJackett}>Convert</button>
  {/snippet}
</Modal>
