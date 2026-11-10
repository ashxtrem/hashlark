<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { Copy, FileDown, FolderOpen, LoaderCircle, Trash2, X } from '@lucide/svelte';
  import FileTree from './FileTree.svelte';
  import type { PeerInfo, TorrentDetails } from '$lib/api/client';
  import { bytes, dateTime } from '$lib/format';
  import { copyText, isTauri, revealPath } from '$lib/platform';
  import { app } from '$lib/state/app.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';
  import { torrents } from '$lib/state/torrents.svelte';
  import { displayPath, fileTree, patch, percent, ratio, selectedIndices, speed, statusLabel } from '$lib/torrents';

  interface Props {
    /** Torrent shown; the panel is closed when null. */
    id: string | null;
    onclose: () => void;
    ondelete: (ids: string[]) => void;
  }

  let { id, onclose, ondelete }: Props = $props();

  type Tab = 'overview' | 'files' | 'peers' | 'trackers' | 'actions';
  const TABS: { id: Tab; label: string }[] = [
    { id: 'overview', label: 'Overview' },
    { id: 'files', label: 'Files' },
    { id: 'peers', label: 'Peers' },
    { id: 'trackers', label: 'Trackers' },
    { id: 'actions', label: 'Actions' },
  ];

  let tab = $state<Tab>('overview');
  let details = $state<TorrentDetails | null>(null);
  let peers = $state<PeerInfo[] | null>(null);
  let error = $state<string | null>(null);
  let closeButton: HTMLButtonElement | undefined = $state();
  /** The live row, fresher than `details` between reloads. */
  let live = $derived(id ? torrents.list.find((t) => t.id === id) : undefined);

  // Limits form (strings so empty means "default").
  let downKib = $state('');
  let upKib = $state('');
  let peerLimit = $state('');
  let seedRatio = $state('');
  let seedMinutes = $state('');
  let saving = $state(false);

  const str = (n: number | null | undefined) => (n == null ? '' : String(n));
  const num = (s: string) => (s.trim() === '' ? null : Number(s));

  function fillForm(d: TorrentDetails) {
    downKib = str(d.limits.down_kib);
    upKib = str(d.limits.up_kib);
    peerLimit = str(d.peer_limit);
    seedRatio = str(d.seed.ratio);
    seedMinutes = str(d.seed.minutes);
  }

  async function load(fill: boolean) {
    if (!id) return;
    try {
      const d = await app.client.torrentDetails(id);
      if (d.id !== id) return;
      details = d;
      error = null;
      if (fill) fillForm(d);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  // Reload while open: details every 2 s, peers when that tab shows.
  $effect(() => {
    if (!id) return;
    details = null;
    peers = null;
    error = null;
    void load(true);
    void Promise.resolve().then(() => closeButton?.focus());
    const timer = setInterval(() => void load(false), 2000);
    return () => clearInterval(timer);
  });

  $effect(() => {
    if (!id || tab !== 'peers') return;
    const current = id;
    const fetchPeers = async () => {
      try {
        peers = await app.client.torrentPeers(current);
      } catch {
        peers = [];
      }
    };
    void fetchPeers();
    const timer = setInterval(fetchPeers, 2000);
    return () => clearInterval(timer);
  });

  let selected = $derived(details ? selectedIndices(details.files) : new Set<number>());
  let tree = $derived(details ? fileTree(details.files) : []);

  async function toggleFiles(indices: number[], checked: boolean) {
    if (!details) return;
    const next = new Set(selected);
    for (const i of indices) {
      if (checked) next.add(i);
      else next.delete(i);
    }
    if (next.size === 0) {
      toasts.error('Keep at least one file selected.');
      return;
    }
    try {
      details = await app.client.updateTorrent(details.id, patch({ files: [...next].sort((a, b) => a - b) }));
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  async function saveLimits(e: SubmitEvent) {
    e.preventDefault();
    if (!details) return;
    saving = true;
    try {
      const peer = num(peerLimit);
      details = await app.client.updateTorrent(
        details.id,
        patch({
          limits: { down_kib: num(downKib), up_kib: num(upKib) },
          // 0 returns to the global per-torrent limit.
          peer_limit: peer === (details.peer_limit ?? null) ? null : (peer ?? 0),
          seed: { ratio: num(seedRatio), minutes: num(seedMinutes) },
        }),
      );
      fillForm(details);
      toasts.success('Limits saved');
    } catch (err) {
      toasts.error(errorMessage(err));
    } finally {
      saving = false;
    }
  }

  async function copy(text: string, what: string) {
    await copyText(text);
    toasts.success(`${what} copied`);
  }

  async function openFolder() {
    if (!details) return;
    const sep = details.save_dir.includes('\\') ? '\\' : '/';
    try {
      await revealPath(`${details.save_dir.replace(/[\\/]+$/, '')}${sep}${details.name}`);
    } catch {
      try {
        await revealPath(details.save_dir);
      } catch (e) {
        toasts.error(errorMessage(e));
      }
    }
  }

  async function saveTorrentFile() {
    if (!details) return;
    try {
      const blob = await app.client.torrentFile(details.id);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${details.name}.torrent`;
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 10_000);
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  let panel: HTMLElement | undefined = $state();
  let open = $derived(id !== null);

  // Focus goes back where it was when the panel closes.
  $effect(() => {
    if (!open) return;
    const back = document.activeElement as HTMLElement | null;
    return () => {
      if (back?.isConnected) back.focus();
    };
  });

  /** Keeps Tab inside the panel (it is modal), from wherever focus is. */
  function trapFocus(e: KeyboardEvent) {
    if (e.key !== 'Tab' || !panel) return;
    const items = [
      ...panel.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ),
    ].filter((el) => el.getClientRects().length > 0);
    const first = items[0];
    const last = items[items.length - 1];
    if (!first || !last) {
      e.preventDefault();
      return;
    }
    const active = document.activeElement;
    if (e.shiftKey && (active === first || !panel.contains(active))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (active === last || !panel.contains(active))) {
      e.preventDefault();
      first.focus();
    }
  }

  function onKey(e: KeyboardEvent) {
    if (!id || document.querySelector('dialog[open]')) return;
    if (e.key === 'Escape') onclose();
    else trapFocus(e);
  }
</script>

<svelte:window onkeydown={onKey} />

{#if id}
  {@const d = details}
  <div class="fixed inset-0 z-30 bg-black/20" aria-hidden="true" onclick={onclose}></div>
  <div
    class="fixed top-0 right-0 bottom-0 z-40 flex w-[min(36rem,100vw)] flex-col border-l border-line bg-surface shadow-card"
    role="dialog"
    aria-modal="true"
    aria-labelledby="torrent-details-title"
    bind:this={panel}
  >
    <header class="border-b border-line px-5 pt-4">
      <div class="flex items-start gap-3">
        <div class="min-w-0 flex-1">
          <h2 id="torrent-details-title" class="text-base leading-snug font-semibold break-words">
            {live?.name ?? d?.name ?? '…'}
          </h2>
          {#if live}
            <p class="mt-1 text-xs text-muted tabular-nums">
              {statusLabel(live)} · {percent(live.progress)} · ↓ {speed(live.down_bps)} · ↑ {speed(live.up_bps)}
            </p>
          {/if}
        </div>
        <button bind:this={closeButton} class="icon-btn -mr-2" aria-label="Close details" onclick={onclose}>
          <X size={18} />
        </button>
      </div>
      <div class="mt-3 -mb-px flex gap-1 overflow-x-auto" role="tablist">
        {#each TABS as t (t.id)}
          <button
            role="tab"
            id="tab-{t.id}"
            aria-selected={tab === t.id}
            aria-controls="tabpanel"
            class="border-b-2 px-3 py-2 text-sm font-medium whitespace-nowrap {tab === t.id
              ? 'border-accent text-accent'
              : 'border-transparent text-muted hover:text-fg'}"
            onclick={() => (tab = t.id)}
          >
            {t.label}
          </button>
        {/each}
      </div>
    </header>

    <div id="tabpanel" role="tabpanel" aria-labelledby="tab-{tab}" class="flex-1 overflow-y-auto px-5 py-4 text-sm">
      {#if error && !d}
        <p class="text-err" role="alert">{error}</p>
      {:else if !d}
        <p class="flex items-center gap-2 text-muted"><LoaderCircle size={16} class="animate-spin" /> Loading…</p>
      {:else if tab === 'overview'}
        {#if d.error}
          <p class="mb-4 rounded-lg bg-err-soft px-3 py-2 text-err" role="alert">{d.error}</p>
        {/if}
        <dl class="grid grid-cols-[8rem_minmax(0,1fr)] gap-x-4 gap-y-2">
          <dt class="text-muted">Save folder</dt>
          <dd class="break-all">{displayPath(d.save_dir)}</dd>
          <dt class="text-muted">Size</dt>
          <dd>{d.size ? `${bytes(d.have)} of ${bytes(d.size)}` : 'Not known yet'}</dd>
          <dt class="text-muted">Info hash</dt>
          <dd class="flex items-center gap-1">
            <code class="truncate font-mono text-xs">{d.id}</code>
            <button class="icon-btn size-6" aria-label="Copy info hash" onclick={() => copy(d.id, 'Info hash')}>
              <Copy size={13} />
            </button>
          </dd>
          <dt class="text-muted">Added</dt>
          <dd>{dateTime(d.added_at) || '–'}</dd>
          <dt class="text-muted">Completed</dt>
          <dd>{dateTime(d.completed_at) || '–'}</dd>
          <dt class="text-muted">Ratio</dt>
          <dd>{ratio(d.ratio)} <span class="text-muted">(↓ {bytes(d.downloaded_total)} · ↑ {bytes(d.uploaded_total)})</span></dd>
          <dt class="text-muted">Peers</dt>
          <dd>{d.peers} connected, {d.seeds} seeds</dd>
          {#if d.magnet}
            <dt class="text-muted">Magnet</dt>
            <dd class="flex items-start gap-1">
              <code class="line-clamp-2 min-w-0 flex-1 font-mono text-xs break-all">{d.magnet}</code>
              <button class="icon-btn size-6 shrink-0" aria-label="Copy magnet link" onclick={() => copy(d.magnet!, 'Magnet link')}>
                <Copy size={13} />
              </button>
            </dd>
          {/if}
        </dl>

        <form class="mt-6 space-y-3" onsubmit={saveLimits}>
          <h3 class="text-xs font-medium tracking-wide text-muted uppercase">Limits for this torrent</h3>
          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="label" for="lim-down">Download (KiB/s)</label>
              <input id="lim-down" class="input" type="number" min="1" placeholder="Unlimited" bind:value={downKib} />
            </div>
            <div>
              <label class="label" for="lim-up">Upload (KiB/s)</label>
              <input id="lim-up" class="input" type="number" min="1" placeholder="Unlimited" bind:value={upKib} />
            </div>
            <div>
              <label class="label" for="lim-peers">Connections</label>
              <input id="lim-peers" class="input" type="number" min="10" max="2000" placeholder="Global default" bind:value={peerLimit} />
            </div>
            <div></div>
            <div>
              <label class="label" for="seed-ratio">Stop seeding at ratio</label>
              <input id="seed-ratio" class="input" type="number" min="0" step="0.1" placeholder="Global default" bind:value={seedRatio} />
            </div>
            <div>
              <label class="label" for="seed-min">or after (minutes)</label>
              <input id="seed-min" class="input" type="number" min="0" placeholder="Global default" bind:value={seedMinutes} />
            </div>
          </div>
          <button class="btn-primary" type="submit" disabled={saving}>Save limits</button>
        </form>
      {:else if tab === 'files'}
        {#if d.files.length === 0}
          <p class="text-muted">The file list appears once the torrent's information has arrived.</p>
        {:else}
          <FileTree nodes={tree} {selected} onchange={toggleFiles} progress />
        {/if}
      {:else if tab === 'peers'}
        {#if peers === null}
          <p class="flex items-center gap-2 text-muted"><LoaderCircle size={16} class="animate-spin" /> Loading…</p>
        {:else if peers.length === 0}
          <p class="text-muted">No peers connected.</p>
        {:else}
          <table class="w-full text-left text-xs">
            <thead class="text-muted">
              <tr><th class="py-1 font-medium">Address</th><th class="font-medium">Client</th><th class="text-right font-medium">↓</th><th class="text-right font-medium">↑</th></tr>
            </thead>
            <tbody class="tabular-nums">
              {#each peers as p (p.addr)}
                <tr class="border-t border-line">
                  <td class="py-1.5 font-mono">{p.addr}{#if p.conn_kind}<span class="ml-1 text-muted">{p.conn_kind}</span>{/if}</td>
                  <td class="truncate">{p.client ?? '–'}</td>
                  <td class="text-right">{bytes(p.downloaded)}</td>
                  <td class="text-right">{bytes(p.uploaded)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      {:else if tab === 'trackers'}
        {#if d.trackers.length === 0}
          <p class="text-muted">No trackers. Peers come from DHT, PEX and local discovery.</p>
        {:else}
          <ul class="space-y-1">
            {#each d.trackers as url (url)}
              <li class="rounded-lg bg-surface-2 px-3 py-1.5 font-mono text-xs break-all">{url}</li>
            {/each}
          </ul>
          <p class="hint mt-3">The engine doesn't report each tracker's status.</p>
        {/if}
      {:else if tab === 'actions'}
        <div class="flex flex-col items-start gap-2">
          {#if isTauri()}
            <button class="btn-outline" onclick={openFolder}><FolderOpen size={15} /> Open folder</button>
          {/if}
          {#if d.size > 0}
            <button class="btn-outline" onclick={saveTorrentFile}><FileDown size={15} /> Save .torrent</button>
          {/if}
          <button class="btn-danger" onclick={() => ondelete([d.id])}><Trash2 size={15} /> Remove…</button>
        </div>
      {/if}
    </div>
  </div>
{/if}
