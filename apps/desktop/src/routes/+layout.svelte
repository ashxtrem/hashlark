<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import '../app.css';
  import { onMount, type Snippet } from 'svelte';
  import { LoaderCircle, TriangleAlert } from '@lucide/svelte';
  import ChallengeDialog from '$components/ChallengeDialog.svelte';
  import LoginScreen from '$components/LoginScreen.svelte';
  import NoHandlerDialog from '$components/NoHandlerDialog.svelte';
  import ResultDetails from '$components/ResultDetails.svelte';
  import Sidebar from '$components/Sidebar.svelte';
  import Toasts from '$components/Toasts.svelte';
  import UpdateBanner from '$components/UpdateBanner.svelte';
  import WelcomeDialog from '$components/WelcomeDialog.svelte';
  import { app } from '$lib/state/app.svelte';
  import { favorites } from '$lib/state/favorites.svelte';
  import { providers } from '$lib/state/providers.svelte';

  let { children }: { children: Snippet } = $props();

  onMount(async () => {
    await app.connect();
    if (app.connection === 'ready') await Promise.all([providers.load(), favorites.load()]);
  });
</script>

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
  <ChallengeDialog />
  <WelcomeDialog />
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
