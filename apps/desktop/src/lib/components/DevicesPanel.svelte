<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { Link2, Monitor, Smartphone, Trash2, Wifi, X } from '@lucide/svelte';
  import Switch from './Switch.svelte';
  import { formatCountdown, formatPin } from '$lib/lan';
  import { lan } from '$lib/state/lan.svelte';

  let now = $state(Date.now());

  onMount(() => {
    void lan.load();
    const tick = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(tick);
  });

  let status = $derived(lan.status);
  let pairing = $derived(status?.pairing ?? null);
  let remaining = $derived(pairing ? pairing.expires_at - now : 0);

  // The engine closes an expired window lazily; hide the PIN on time.
  $effect(() => {
    if (pairing && remaining <= 0) lan.setPairing(null);
  });
</script>

<section class="card space-y-4 p-5">
  <h2 class="flex items-center gap-2 font-semibold"><Wifi size={17} /> Devices on this network</h2>
  <div class="flex items-center justify-between gap-4">
    <div>
      <p class="text-sm font-medium">Share with devices on this network</p>
      <p class="hint">
        Send results to Hashlark on a phone on the same Wi-Fi, and let paired phones send results
        here to open in this computer's torrent client. Only devices you pair can send. Windows may
        ask you to allow Hashlark through the firewall.
      </p>
    </div>
    <Switch
      checked={status?.enabled ?? false}
      disabled={lan.busy || !status}
      label="Share with devices on this network"
      onchange={(on) => lan.setEnabled(on)}
    />
  </div>

  {#if status?.error}
    <p class="text-sm text-err" role="alert">Couldn't start: {status.error}</p>
  {/if}

  {#if status?.running}
    <div class="text-sm">
      <p>This computer appears as <strong>{status.name}</strong>.</p>
      {#if status.addresses.length}
        <p class="hint">
          If your phone doesn't list it, enter
          {#each status.addresses as addr, i (addr)}{#if i > 0}{' or '}{/if}<code>{addr}</code>{/each}
          on the phone.
        </p>
      {/if}
      {#if status.discovery_error}
        <p class="hint text-warn">
          Automatic discovery isn't working on this computer, so enter the address on the phone.
        </p>
      {/if}
    </div>

    {#if pairing}
      <div class="rounded-xl border border-accent bg-accent-soft p-4 text-center" role="status" aria-live="polite">
        <p class="text-sm">
          On your phone, choose <strong>Send to…</strong>, pick <strong>{status.name}</strong> and
          enter this PIN:
        </p>
        <p class="my-3 font-mono text-4xl font-semibold tracking-widest" aria-label="PIN {pairing.pin.split('').join(' ')}">
          {formatPin(pairing.pin)}
        </p>
        <p class="hint">Expires in {formatCountdown(remaining)}</p>
        <button type="button" class="btn-ghost mt-2" onclick={() => lan.closePairing()}>
          <X size={15} /> Cancel
        </button>
      </div>
    {:else}
      <button type="button" class="btn-outline" onclick={() => lan.openPairing()}>
        <Link2 size={15} /> Pair a phone
      </button>
    {/if}

    <div>
      <h3 class="label">Paired devices</h3>
      {#if lan.paired.length === 0}
        <p class="hint">None yet.</p>
      {:else}
        <ul class="divide-y divide-line text-sm">
          {#each lan.paired as device (device.id)}
            <li class="flex items-center gap-3 py-2">
              {#if device.platform === 'android'}
                <Smartphone size={16} class="text-muted" />
              {:else}
                <Monitor size={16} class="text-muted" />
              {/if}
              <span class="flex-1 font-medium">{device.name}</span>
              <span class="text-xs {device.online ? 'text-ok' : 'text-muted'}">
                {device.online ? 'On this network' : 'Not seen'}
              </span>
              <button
                class="icon-btn text-err"
                type="button"
                aria-label="Forget {device.name}"
                onclick={() => lan.forget(device)}
              >
                <Trash2 size={15} />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</section>
