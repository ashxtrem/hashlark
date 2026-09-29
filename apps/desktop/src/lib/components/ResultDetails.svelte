<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { Copy, Download, ExternalLink, FileDown, Star, X } from '@lucide/svelte';
  import { copyMagnet, download, linkKinds, saveTorrent } from '$lib/actions';
  import { bytes, CATEGORY_LABELS, count, dateTime } from '$lib/format';
  import { copyText, openExternal } from '$lib/platform';
  import { favorites } from '$lib/state/favorites.svelte';
  import { toasts } from '$lib/state/toasts.svelte';
  import { ui } from '$lib/state/ui.svelte';

  interface Props {
    names: Record<string, string>;
  }

  let { names }: Props = $props();
  let result = $derived(ui.details);
  let closeButton: HTMLButtonElement | undefined = $state();

  $effect(() => {
    if (result) closeButton?.focus();
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && ui.details) ui.details = null;
  }

  async function copyHash(hash: string) {
    await copyText(hash);
    toasts.success('Infohash copied');
  }
</script>

<svelte:window onkeydown={onKey} />

{#if result}
  {@const p = result.primary}
  {@const links = linkKinds(result)}
  <div class="fixed inset-0 z-30 bg-black/20" aria-hidden="true" onclick={() => (ui.details = null)}></div>
  <div
    class="fixed top-0 right-0 bottom-0 z-40 flex w-[min(30rem,100vw)] flex-col border-l border-line bg-surface shadow-card"
    role="dialog"
    aria-modal="true"
    aria-labelledby="details-title"
  >
    <header class="flex items-start gap-3 border-b border-line px-5 py-4">
      <h2 id="details-title" class="min-w-0 flex-1 text-base leading-snug font-semibold break-words">{p.title}</h2>
      <button bind:this={closeButton} class="icon-btn -mr-2" aria-label="Close details" onclick={() => (ui.details = null)}>
        <X size={18} />
      </button>
    </header>

    <div class="flex-1 space-y-5 overflow-y-auto px-5 py-4 text-sm">
      <div class="flex flex-wrap gap-2">
        <button class="btn-primary" onclick={() => download(result)}><Download size={16} /> Open in client</button>
        <button class="btn-outline" disabled={!links.magnet} onclick={() => copyMagnet(result)}><Copy size={16} /> Copy magnet</button>
        <button class="btn-outline" disabled={!links.torrent} onclick={() => saveTorrent(result)}><FileDown size={16} /> .torrent</button>
        <button class="btn-outline" aria-pressed={favorites.has(result.id)} onclick={() => favorites.toggle(result)}>
          <Star size={16} fill={favorites.has(result.id) ? 'currentColor' : 'none'} class={favorites.has(result.id) ? 'text-warn' : ''} />
          {favorites.has(result.id) ? 'Saved' : 'Save'}
        </button>
      </div>

      <dl class="grid grid-cols-[7rem_1fr] gap-x-4 gap-y-2">
        <dt class="text-muted">Size</dt>
        <dd>{bytes(p.size_bytes)}</dd>
        <dt class="text-muted">Seeds / peers</dt>
        <dd>{count(result.seeders)} / {count(result.leechers)}</dd>
        <dt class="text-muted">Published</dt>
        <dd>{dateTime(p.published) || '–'}</dd>
        <dt class="text-muted">Category</dt>
        <dd>{p.category ? CATEGORY_LABELS[p.category] : '–'}</dd>
        {#if p.info_hash}
          <dt class="text-muted">Infohash</dt>
          <dd class="flex items-center gap-1">
            <code class="truncate font-mono text-xs">{p.info_hash}</code>
            <button class="icon-btn size-6" aria-label="Copy infohash" onclick={() => copyHash(p.info_hash!)}>
              <Copy size={13} />
            </button>
          </dd>
        {/if}
      </dl>

      <section>
        <h3 class="mb-2 text-xs font-medium tracking-wide text-muted uppercase">Found on</h3>
        <ul class="space-y-1.5">
          {#each result.sources as source (source)}
            <li class="flex items-center justify-between gap-2 rounded-lg bg-surface-2 px-3 py-2">
              <span>{names[source] ?? source}</span>
              {#if source === p.provider_id && p.details_url}
                <button class="btn-ghost -my-1 px-2 py-1 text-xs" onclick={() => openExternal(p.details_url!)}>
                  View on site <ExternalLink size={13} />
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    </div>
  </div>
{/if}
