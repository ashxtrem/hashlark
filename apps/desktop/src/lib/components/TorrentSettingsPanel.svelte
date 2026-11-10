<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { FolderOpen } from '@lucide/svelte';
  import type { Settings } from '$lib/api/client';
  import ScheduleGrid from './ScheduleGrid.svelte';
  import Switch from './Switch.svelte';
  import {
    isTauri,
    magnetHandlerRegister,
    magnetHandlerStatus,
    pickFolder,
    type MagnetHandlerStatus,
  } from '$lib/platform';
  import { filled } from '$lib/schedule';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';

  interface Props {
    /** The `torrents` part of the settings being edited; saved with the page. */
    t: Settings['torrents'];
  }

  let { t = $bindable() }: Props = $props();

  const desktop = isTauri();

  let handler = $state<MagnetHandlerStatus | null>(null);
  let isSelf = $state(false);
  let handlerBusy = $state(false);

  function setStatus(s: MagnetHandlerStatus | null) {
    handler = s;
    isSelf = s?.is_self ?? false;
  }

  $effect(() => {
    if (!desktop) return;
    magnetHandlerStatus()
      .then(setStatus)
      .catch(() => {});
  });

  /** Applies at once (it is an OS setting, not part of the saved settings). */
  async function setHandler(register: boolean) {
    handlerBusy = true;
    try {
      setStatus(await magnetHandlerRegister(register));
    } catch (e) {
      toasts.error(errorMessage(e));
      setStatus(await magnetHandlerStatus().catch(() => handler));
    } finally {
      handlerBusy = false;
    }
  }

  async function browse(field: 'download_dir' | 'watch_dir') {
    const folder = await pickFolder();
    if (folder) t[field] = folder;
  }

  function setSchedule(on: boolean) {
    t.schedule = on ? (t.schedule ?? filled('n')) : null;
  }
</script>

<section class="card space-y-5 p-5" aria-labelledby="torrent-client-title">
  <h2 id="torrent-client-title" class="font-semibold">Torrent client</h2>

  <div class="flex items-center justify-between gap-4">
    <div>
      <p class="text-sm font-medium">Download with Hashlark</p>
      <p class="hint">
        Turn off to send results to another torrent client on this computer instead.
      </p>
    </div>
    <Switch bind:checked={t.enabled} label="Use the built-in torrent client" />
  </div>

  <div>
    <span class="label">Download folder</span>
    <div class="flex gap-2">
      <input
        class="input"
        aria-label="Download folder"
        placeholder="Default (Downloads/Hashlark)"
        bind:value={t.download_dir}
      />
      {#if desktop}
        <button type="button" class="btn-outline shrink-0" onclick={() => browse('download_dir')}>
          <FolderOpen size={15} /> Browse
        </button>
      {/if}
    </div>
  </div>

  <div class="flex items-center justify-between gap-4">
    <div>
      <p class="text-sm font-medium">Ask where to save each torrent</p>
      <p class="hint">Shows the Add dialog every time. Shift+click on Download does this for one torrent.</p>
    </div>
    <Switch bind:checked={t.ask_on_add} label="Ask on add" />
  </div>

  <div class="space-y-4 border-t border-line pt-4">
    <h3 class="text-sm font-semibold">Network</h3>
    <label class="block max-w-sm">
      <span class="label">Listen port</span>
      <input class="input" type="number" min="0" max="65535" required bind:value={t.listen_port} />
      <span class="hint">0 picks one on first start. Otherwise 1024–65535.</span>
    </label>
    <div class="grid gap-3 sm:grid-cols-2">
      <div class="flex items-center justify-between gap-4">
        <p class="text-sm">UPnP port mapping</p>
        <Switch bind:checked={t.upnp} label="UPnP" />
      </div>
      <div class="flex items-center justify-between gap-4">
        <p class="text-sm">DHT (find peers without a tracker)</p>
        <Switch bind:checked={t.dht} label="DHT" />
      </div>
      <div class="flex items-center justify-between gap-4">
        <p class="text-sm">Local peer discovery</p>
        <Switch bind:checked={t.lsd} label="Local peer discovery" />
      </div>
      <div class="flex items-center justify-between gap-4">
        <p class="text-sm">uTP (UDP transport)</p>
        <Switch bind:checked={t.utp} label="uTP" />
      </div>
    </div>
  </div>

  <div class="space-y-4 border-t border-line pt-4">
    <h3 class="text-sm font-semibold">Speed limits</h3>
    <div class="grid gap-4 sm:grid-cols-4">
      <label>
        <span class="label">Download (KiB/s)</span>
        <input class="input" type="number" min="0" placeholder="Unlimited" bind:value={t.limits.down_kib} />
      </label>
      <label>
        <span class="label">Upload (KiB/s)</span>
        <input class="input" type="number" min="0" placeholder="Unlimited" bind:value={t.limits.up_kib} />
      </label>
      <label>
        <span class="label">Alternative download</span>
        <input class="input" type="number" min="0" placeholder="Unlimited" bind:value={t.alt_limits.down_kib} />
      </label>
      <label>
        <span class="label">Alternative upload</span>
        <input class="input" type="number" min="0" placeholder="Unlimited" bind:value={t.alt_limits.up_kib} />
      </label>
    </div>
    <p class="hint">
      Empty means unlimited. The alternative limits apply in turtle mode, in the schedule's Alternative hours and
      on battery if you choose that below.
    </p>
  </div>

  <div class="space-y-4 border-t border-line pt-4">
    <div class="flex items-center justify-between gap-4">
      <div>
        <h3 class="text-sm font-semibold">Weekly schedule</h3>
        <p class="hint">Pick a mode for each hour of the week: normal, alternative speed, seed only or off.</p>
      </div>
      <Switch checked={t.schedule !== null} onchange={setSchedule} label="Use a weekly schedule" />
    </div>
    {#if t.schedule !== null}
      <ScheduleGrid bind:value={t.schedule} />
    {/if}
  </div>

  <div class="space-y-4 border-t border-line pt-4">
    <h3 class="text-sm font-semibold">Connections</h3>
    <div class="grid gap-4 sm:grid-cols-2">
      <label>
        <span class="label">Peers in total</span>
        <input class="input" type="number" min="20" max="10000" required bind:value={t.peer_limit_global} />
        <span class="hint">20–10000, and not below the per-torrent limit.</span>
      </label>
      <label>
        <span class="label">Peers per torrent</span>
        <input class="input" type="number" min="10" max="2000" required bind:value={t.peer_limit_torrent} />
        <span class="hint">10–2000. Each torrent can override this.</span>
      </label>
    </div>
  </div>

  <div class="space-y-4 border-t border-line pt-4">
    <h3 class="text-sm font-semibold">Queue and seeding</h3>
    <div class="grid gap-4 sm:grid-cols-4">
      <label>
        <span class="label">Active downloads</span>
        <input class="input" type="number" min="1" max="100" required bind:value={t.max_active_downloads} />
      </label>
      <label>
        <span class="label">Active seeds</span>
        <input class="input" type="number" min="1" max="100" required bind:value={t.max_active_seeds} />
      </label>
      <label>
        <span class="label">Stop at ratio</span>
        <input
          class="input"
          type="number"
          min="0"
          max="100"
          step="0.1"
          placeholder="No limit"
          bind:value={t.seed_ratio}
        />
      </label>
      <label>
        <span class="label">Stop after (minutes)</span>
        <input class="input" type="number" min="0" placeholder="No limit" bind:value={t.seed_minutes} />
      </label>
    </div>
    <p class="hint">
      Torrents over the active limits wait in the queue. Seeding stops at the first limit reached; empty means no
      limit. Each torrent can override these.
    </p>
  </div>

  {#if desktop}
    <div class="space-y-4 border-t border-line pt-4">
      <h3 class="text-sm font-semibold">Power</h3>
      <div class="grid gap-4 sm:grid-cols-2">
        <label>
          <span class="label">Keep the computer awake</span>
          <select class="input" bind:value={t.keep_awake}>
            <option value="off">Off</option>
            <option value="downloading">While downloading</option>
            <option value="active">While downloading or seeding</option>
          </select>
        </label>
        <label>
          <span class="label">On battery</span>
          <select class="input" bind:value={t.on_battery}>
            <option value="nothing">Do nothing</option>
            <option value="alt_speed">Alternative speed</option>
            <option value="pause">Pause all</option>
          </select>
        </label>
      </div>
    </div>
  {/if}

  <div class="space-y-4 border-t border-line pt-4">
    <h3 class="text-sm font-semibold">Privacy and filtering</h3>
    <label class="block">
      <span class="label">SOCKS5 proxy for torrents</span>
      <input class="input" placeholder="socks5://127.0.0.1:1080" bind:value={t.proxy} />
      <span class="hint">
        Peer connections go through it. DHT and uTP still connect directly, because they use UDP. Leave empty for
        none. Downloads never use Tor or the proxy in Network &amp; privacy.
      </span>
    </label>
    <label class="block">
      <span class="label">IP blocklist URL</span>
      <input class="input" placeholder="https://example.org/blocklist.p2p.gz" bind:value={t.blocklist_url} />
      <span class="hint">Optional. Peers on the list are refused.</span>
    </label>
    <div>
      <span class="label">Watched folder</span>
      <div class="flex gap-2">
        <input
          class="input"
          aria-label="Watched folder"
          placeholder="None"
          bind:value={t.watch_dir}
        />
        {#if desktop}
          <button type="button" class="btn-outline shrink-0" onclick={() => browse('watch_dir')}>
            <FolderOpen size={15} /> Browse
          </button>
        {/if}
      </div>
      <p class="hint">
        .torrent files put in this folder are added automatically, then renamed to .torrent.added.
      </p>
    </div>
  </div>

  {#if desktop}
    <div class="space-y-4 border-t border-line pt-4">
      <h3 class="text-sm font-semibold">This computer</h3>
      <div class="flex items-center justify-between gap-4">
        <div>
          <p class="text-sm font-medium">Open magnet links with Hashlark</p>
          <p class="hint" role="status">
            {#if handler === null}
              Not available on this system.
            {:else if handler.is_self}
              Hashlark opens magnet links from your browser and other apps.
            {:else if handler.registered}
              Another app opens magnet links now.
            {:else}
              No app opens magnet links yet.
            {/if}
          </p>
        </div>
        <Switch
          bind:checked={isSelf}
          disabled={handler === null || handlerBusy}
          onchange={setHandler}
          label="Open magnet links with Hashlark"
        />
      </div>
      <div class="flex items-center justify-between gap-4">
        <div>
          <p class="text-sm font-medium">Keep running in the tray</p>
          <p class="hint">Closing the window keeps downloads going. Quit from the tray menu.</p>
        </div>
        <Switch bind:checked={t.close_to_tray} label="Keep running in the tray" />
      </div>
    </div>
  {/if}
</section>
