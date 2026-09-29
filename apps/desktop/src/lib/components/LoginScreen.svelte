<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { storeToken } from '$lib/api/client';
  import { app } from '$lib/state/app.svelte';

  let token = $state('');
  let failed = $state(false);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    storeToken(token.trim());
    await app.connect();
    failed = app.connection !== 'ready';
    if (failed) storeToken(null);
  }
</script>

<div class="flex min-h-screen items-center justify-center p-6">
  <form class="card w-full max-w-sm space-y-4 p-6" onsubmit={submit}>
    <div class="flex items-center gap-2.5">
      <img src="/favicon.svg" alt="" class="size-9" />
      <h1 class="text-xl font-semibold">Hashlark</h1>
    </div>
    <p class="text-sm text-muted">
      This Hashlark server needs an API key. Find it in the server's log on first start, or create one
      in Settings → API keys on a device that's already signed in.
    </p>
    <div>
      <label class="label" for="token">API key</label>
      <input id="token" class="input font-mono" type="password" autocomplete="current-password" bind:value={token} required />
      {#if failed}<p class="mt-1 text-xs text-err" role="alert">That key was not accepted.</p>{/if}
    </div>
    <button class="btn-primary w-full" type="submit" disabled={!token.trim()}>Sign in</button>
  </form>
</div>
