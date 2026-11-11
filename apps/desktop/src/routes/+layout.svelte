<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import '../app.css';
  import { onMount, untrack, type Snippet } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { LoaderCircle, TriangleAlert } from '@lucide/svelte';
  import AddTorrentDialog from '$components/AddTorrentDialog.svelte';
  import ChallengeDialog from '$components/ChallengeDialog.svelte';
  import CreateTorrentDialog from '$components/CreateTorrentDialog.svelte';
  import DevicePicker from '$components/DevicePicker.svelte';
  import LoginScreen from '$components/LoginScreen.svelte';
  import NoHandlerDialog from '$components/NoHandlerDialog.svelte';
  import ResultDetails from '$components/ResultDetails.svelte';
  import Sidebar from '$components/Sidebar.svelte';
  import Toasts from '$components/Toasts.svelte';
  import TorrentNotice from '$components/TorrentNotice.svelte';
  import UpdateBanner from '$components/UpdateBanner.svelte';
  import WelcomeDialog from '$components/WelcomeDialog.svelte';
  import WhenDoneDialog from '$components/WhenDoneDialog.svelte';
  import { onLinkOpen } from '$lib/platform';
  import { parseSearchLink } from '$lib/searchLink';
  import { app } from '$lib/state/app.svelte';
  import { favorites } from '$lib/state/favorites.svelte';
  import { lan } from '$lib/state/lan.svelte';
  import { providers } from '$lib/state/providers.svelte';
  import { search } from '$lib/state/search.svelte';
  import { torrents } from '$lib/state/torrents.svelte';

  let { children }: { children: Snippet } = $props();
  /** A file is dragged over the window. */
  let dropping = $state(false);

  // Also after a login in the web UI, which connects after mount.
  $effect(() => {
    if (app.connection === 'ready') untrack(() => torrents.start());
  });

  onMount(async () => {
    await app.connect();
    if (app.connection === 'ready') {
      await Promise.all([providers.load(), favorites.load(), lan.listen()]);
      await onLinkOpen(openLink);
    }
  });

  /**
   * Runs the search of a `hashlark://search` link, like typing it on the
   * search page. Nothing else in the link is used. A link without text only
   * brings the window forward, which the Rust side does.
   */
  function openLink(url: string) {
    const link = parseSearchLink(url);
    if (!link?.q) return;
    search.open(link);
    const qs = new URLSearchParams({ q: link.q });
    if (link.categories.length) qs.set('cat', link.categories.join(','));
    void goto(`/?${qs}`, { replaceState: page.url.pathname === '/', keepFocus: true, noScroll: true });
    void search.run(app.client);
  }

  const hasFiles = (e: DragEvent) => e.dataTransfer?.types.includes('Files') ?? false;

  /** `.torrent` files dropped anywhere on the window are added. */
  function onDrop(e: DragEvent) {
    dropping = false;
    if (!hasFiles(e) || app.connection !== 'ready') return;
    e.preventDefault();
    const files = [...(e.dataTransfer?.files ?? [])];
    if (files.length) void torrents.addFiles(files, e.shiftKey);
  }
</script>

<svelte:window
  ondragover={(e) => {
    if (!hasFiles(e) || app.connection !== 'ready') return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
    dropping = true;
  }}
  ondragleave={(e) => {
    if (!e.relatedTarget) dropping = false;
  }}
  ondrop={onDrop}
/>

{#if app.connection === 'ready'}
  <a href="#content" class="sr-only z-50 rounded-lg bg-accent px-3 py-2 text-accent-fg focus:not-sr-only focus:fixed focus:top-2 focus:left-2">Skip to content</a>
  <div class="flex h-screen overflow-hidden">
    <Sidebar />
    <main id="content" tabindex="-1" class="min-w-0 flex-1 overflow-y-auto focus:outline-none">
      <UpdateBanner />
      {@render children()}
    </main>
  </div>
  <ResultDetails names={providers.names} />
  <NoHandlerDialog />
  <DevicePicker />
  <ChallengeDialog />
  <WelcomeDialog />
  <TorrentNotice />
  <AddTorrentDialog />
  <CreateTorrentDialog />
  <WhenDoneDialog />
  {#if dropping}
    <div class="pointer-events-none fixed inset-3 z-50 flex items-center justify-center rounded-2xl border-2 border-dashed border-accent bg-accent-soft/80 text-lg font-medium text-accent">
      Drop .torrent files to add them
    </div>
  {/if}
{:else if app.connection === 'needs_login'}
  <LoginScreen />
{:else if app.connection === 'error'}
  <div class="flex min-h-screen items-center justify-center p-6">
    <div class="card max-w-md space-y-3 p-6 text-center">
      <TriangleAlert size={28} class="mx-auto text-err" />
      <h1 class="text-lg font-semibold">Can't reach the Hashlark engine</h1>
      <p class="text-sm text-muted">{app.error}</p>
      <button class="btn-primary" onclick={() => app.connect()}>Try again</button>
    </div>
  </div>
{:else}
  <div class="flex min-h-screen items-center justify-center gap-2 text-muted" role="status">
    <LoaderCircle size={18} class="animate-spin" /> Starting…
  </div>
{/if}

<Toasts />
