<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { FolderOpen, Monitor, Moon, RefreshCw, Sun } from '@lucide/svelte';
  import type { Settings } from '$lib/api/client';
  import ApiKeysPanel from '$components/ApiKeysPanel.svelte';
  import Switch from '$components/Switch.svelte';
  import { isTauri, pickFolder } from '$lib/platform';
  import { app } from '$lib/state/app.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';

  // Edit a copy; nothing changes until Save.
  const initial = structuredClone($state.snapshot(app.settings!)) as Settings;
  let draft = $state<Settings>(structuredClone(initial));
  let saving = $state(false);
  let error = $state<string | null>(null);
  let proxy = $state(initial.network.proxy ?? '');
  let trackersUrl = $state(initial.magnets.trackers_url ?? '');
  let customDoh = $state(initial.network.doh.custom_url ?? '');
  let torUrl = $state(initial.network.tor.socks_url ?? '');

  let dirty = $derived(
    JSON.stringify(withTextFields(draft)) !== JSON.stringify(app.settings),
  );

  function withTextFields(s: Settings): Settings {
    return {
      ...s,
      network: {
        ...s.network,
        proxy: proxy.trim() || null,
        doh: { ...s.network.doh, custom_url: customDoh.trim() || null },
        tor: { ...s.network.tor, socks_url: torUrl.trim() || null },
      },
      magnets: { ...s.magnets, trackers_url: trackersUrl.trim() || null },
    };
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    saving = true;
    error = null;
    try {
      const saved = await app.saveSettings(withTextFields($state.snapshot(draft) as Settings));
      draft = structuredClone(saved);
      toasts.success('Settings saved');
    } catch (err) {
      error = errorMessage(err);
    } finally {
      saving = false;
    }
  }

  function reset() {
    draft = structuredClone($state.snapshot(app.settings!));
    proxy = draft.network.proxy ?? '';
    trackersUrl = draft.magnets.trackers_url ?? '';
    customDoh = draft.network.doh.custom_url ?? '';
    torUrl = draft.network.tor.socks_url ?? '';
    error = null;
  }

  let refreshing = $state(false);
  let torStatus = $state<import('$lib/api/client').Schemas['TorStatus'] | null>(null);

  // Show built-in Tor's progress while this page is open.
  $effect(() => {
    let stopped = false;
    const poll = async () => {
      try {
        torStatus = await app.client.torStatus();
      } catch {
        // Status is informational only.
      }
      if (!stopped && torStatus?.running && !torStatus.ready) setTimeout(poll, 2000);
    };
    poll();
    return () => {
      stopped = true;
    };
  });

  async function refreshTrackers() {
    refreshing = true;
    try {
      const n = await app.client.refreshTrackers();
      toasts.success(`Tracker list updated: ${n} trackers`);
    } catch (err) {
      toasts.error(errorMessage(err));
    } finally {
      refreshing = false;
    }
  }

  async function browse() {
    const folder = await pickFolder();
    if (folder) draft.downloads.torrent_dir = folder;
  }

  const THEMES = [
    { value: 'system', label: 'System', icon: Monitor },
    { value: 'light', label: 'Light', icon: Sun },
    { value: 'dark', label: 'Dark', icon: Moon },
  ] as const;
</script>

<svelte:head><title>Settings – Hashlark</title></svelte:head>

<form class="mx-auto max-w-3xl space-y-5 px-6 py-6 pb-24" onsubmit={save}>
  <h1 class="text-2xl font-semibold">Settings</h1>

  <section class="card space-y-4 p-5">
    <h2 class="font-semibold">Search</h2>
    <div class="grid gap-4 sm:grid-cols-3">
      <label>
        <span class="label">Provider timeout (s)</span>
        <input class="input" type="number" min="1" max="120" bind:value={draft.search.provider_timeout_secs} />
      </label>
      <label>
        <span class="label">Providers at once</span>
        <input class="input" type="number" min="1" max="64" bind:value={draft.search.max_concurrency} />
      </label>
      <label>
        <span class="label">Cache results (s)</span>
        <input class="input" type="number" min="0" max="86400" bind:value={draft.search.cache_ttl_secs} />
        <span class="hint">0 turns caching off.</span>
      </label>
    </div>
    <div class="flex items-center justify-between gap-4">
      <div>
        <p class="text-sm font-medium">Save search history</p>
        <p class="hint">Kept only on this device.</p>
      </div>
      <Switch bind:checked={draft.search.save_history} label="Save search history" />
    </div>
  </section>

  <section class="card space-y-4 p-5">
    <h2 class="font-semibold">Network &amp; privacy</h2>
    <div class="flex items-center justify-between gap-4">
      <div>
        <p class="text-sm font-medium">Encrypted DNS (DNS-over-HTTPS)</p>
        <p class="hint">Looks up provider addresses privately instead of through your network's DNS.</p>
      </div>
      <Switch bind:checked={draft.network.doh.enabled} label="Encrypted DNS" />
    </div>
    {#if draft.network.doh.enabled}
      <div class="grid gap-4 sm:grid-cols-2">
        <label>
          <span class="label">DNS provider</span>
          <select class="input" bind:value={draft.network.doh.resolver}>
            <option value="cloudflare">Cloudflare (1.1.1.1)</option>
            <option value="quad9">Quad9</option>
            <option value="google">Google</option>
            <option value="custom">Custom…</option>
          </select>
        </label>
        {#if draft.network.doh.resolver === 'custom'}
          <label>
            <span class="label">DoH URL</span>
            <input class="input" placeholder="https://dns.example/dns-query" bind:value={customDoh} />
          </label>
        {/if}
      </div>
      <div class="flex items-center justify-between gap-4">
        <p class="text-sm">Fall back to system DNS if encrypted DNS fails</p>
        <Switch bind:checked={draft.network.doh.fallback_to_system} label="Fall back to system DNS" />
      </div>
    {/if}
    <label class="block">
      <span class="label">Proxy</span>
      <input class="input" placeholder="socks5h://127.0.0.1:9050 or http://proxy:8080" bind:value={proxy} />
      <span class="hint">Used for every provider unless a provider has its own network setting. Leave empty for none.</span>
    </label>
    <div class="flex items-center justify-between gap-4">
      <div>
        <p class="text-sm font-medium">Route everything through Tor</p>
        <p class="hint">Needs Tor running on this computer (the Tor service, Tor Browser or Orbot). Slower, but reaches sites blocked on your network and <code>.onion</code> mirrors.</p>
      </div>
      <Switch bind:checked={draft.network.tor.enabled} label="Use Tor" />
    </div>
    <div class="grid gap-4 sm:grid-cols-2">
      <label>
        <span class="label">Tor</span>
        <select class="input" bind:value={draft.network.tor.mode}>
          <option value="embedded">Built into Hashlark</option>
          <option value="external">My own Tor (SOCKS address)</option>
        </select>
      </label>
      {#if draft.network.tor.mode === 'external'}
        <label>
          <span class="label">Tor SOCKS address</span>
          <input class="input" placeholder="socks5h://127.0.0.1:9050" bind:value={torUrl} />
          <span class="hint">Tor service / Orbot: port 9050. Tor Browser: port 9150.</span>
        </label>
      {:else if torStatus}
        <p class="self-end text-sm {torStatus.ready ? 'text-ok' : 'text-muted'}" role="status">
          {#if !torStatus.built_in}This build has no built-in Tor.
          {:else if !torStatus.running}Starts when Tor is turned on or a provider uses it.
          {:else if torStatus.ready}Connected to Tor.
          {:else}Connecting to Tor… {Math.round(torStatus.progress * 100)}%{/if}
        </p>
      {/if}
    </div>
    <p class="hint">Providers can also use Tor individually (Providers → Details → Route).</p>
    <label class="block max-w-xs">
      <span class="label">Requests per second to one site</span>
      <input class="input" type="number" min="1" max="20" bind:value={draft.network.per_host_rate} />
    </label>
  </section>

  <section class="card space-y-4 p-5">
    <h2 class="font-semibold">Magnet links</h2>
    <div class="flex items-center justify-between gap-4">
      <div>
        <p class="text-sm font-medium">Add public trackers to every magnet</p>
        <p class="hint">Can help find peers faster. Magnets without trackers always get them.</p>
      </div>
      <Switch bind:checked={draft.magnets.append_default_trackers} label="Add public trackers" />
    </div>
    <label class="block">
      <span class="label">Tracker list URL</span>
      <input class="input" placeholder="https://example.org/trackers.txt" bind:value={trackersUrl} />
      <span class="hint">Optional plain-text list, one tracker per line, refreshed daily.</span>
    </label>
    {#if app.settings?.magnets.trackers_url}
      <button type="button" class="btn-outline" disabled={refreshing} onclick={refreshTrackers}>
        <RefreshCw size={15} class={refreshing ? 'animate-spin' : ''} /> Refresh tracker list now
      </button>
    {/if}
  </section>

  <section class="card space-y-4 p-5">
    <h2 class="font-semibold">Downloads</h2>
    <div>
      <span class="label">Save .torrent files to</span>
      <div class="flex gap-2">
        <input class="input" placeholder="Your Downloads folder" bind:value={draft.downloads.torrent_dir} />
        {#if isTauri()}
          <button type="button" class="btn-outline shrink-0" onclick={browse}><FolderOpen size={15} /> Browse</button>
        {/if}
      </div>
      <p class="hint">Magnet links go straight to the torrent client your system uses for them.</p>
    </div>
  </section>

  <section class="card space-y-4 p-5">
    <h2 class="font-semibold">Appearance &amp; updates</h2>
    <div role="radiogroup" aria-label="Theme" class="flex gap-2">
      {#each THEMES as t (t.value)}
        <button
          type="button"
          role="radio"
          aria-checked={draft.ui.theme === t.value}
          class="btn-outline {draft.ui.theme === t.value ? 'border-accent bg-accent-soft text-accent' : ''}"
          onclick={() => (draft.ui.theme = t.value)}
        >
          <t.icon size={15} /> {t.label}
        </button>
      {/each}
    </div>
    <div class="flex items-center justify-between gap-4">
      <div>
        <p class="text-sm font-medium">Check for updates</p>
        <p class="hint">Asks GitHub Releases for a newer version. Nothing else is sent.</p>
      </div>
      <Switch bind:checked={draft.updates.check_for_updates} label="Check for updates" />
    </div>
  </section>

  <section class="card space-y-2 p-5 text-sm">
    <h2 class="font-semibold">About</h2>
    <p>Hashlark {app.version} · Licensed under GPL-3.0-or-later.</p>
    <p class="text-muted">
      No telemetry: Hashlark sends no analytics, crash reports or usage data. It only contacts the
      providers you enable, definition repositories you add, your tracker list URL, and GitHub for the
      update check.
    </p>
    <p class="text-muted">
      Hashlark is a neutral search tool and hosts no content. You are responsible for complying with
      the law and copyright where you live.
    </p>
  </section>

  {#if dirty || error}
    <div class="fixed right-6 bottom-6 left-58 z-20 mx-auto flex max-w-3xl items-center gap-3 rounded-xl border border-line bg-surface px-4 py-3 shadow-card">
      {#if error}
        <p class="flex-1 text-sm text-err" role="alert">{error}</p>
      {:else}
        <p class="flex-1 text-sm text-muted">You have unsaved changes.</p>
      {/if}
      <button type="button" class="btn-ghost" onclick={reset}>Discard</button>
      <button type="submit" class="btn-primary" disabled={saving}>Save changes</button>
    </div>
  {/if}
</form>

{#if !isTauri()}
  <!-- Outside the settings form: it has its own form. -->
  <div class="mx-auto max-w-3xl px-6 pb-24">
    <ApiKeysPanel />
  </div>
{/if}
