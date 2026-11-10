<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { untrack } from 'svelte';
  import { ArrowLeft, LoaderCircle, Monitor, Pencil, Smartphone } from '@lucide/svelte';
  import Modal from './Modal.svelte';
  import { isPin, lanErrorMessage, pickerSections, pinHint } from '$lib/lan';
  import { isLanError, lanApi, type LanDevice } from '$lib/platform';
  import { lan } from '$lib/state/lan.svelte';
  import { toasts } from '$lib/state/toasts.svelte';
  import { ui } from '$lib/state/ui.svelte';

  type Step = { kind: 'list' } | { kind: 'pin'; device: LanDevice } | { kind: 'address' };

  let step = $state<Step>({ kind: 'list' });
  /** Id of the device being paired with or sent to, or 'address'. */
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let pin = $state('');
  let address = $state('');

  let target = $derived(ui.sendTarget);
  let sections = $derived(pickerSections(lan.devices));

  // Start from the list, with a fresh device list, every time it opens.
  $effect(() => {
    if (!target) return;
    untrack(() => {
      step = { kind: 'list' };
      error = null;
      pin = '';
      address = '';
      void lan.load();
    });
  });

  function close() {
    ui.sendTarget = null;
  }

  function askPin(device: LanDevice) {
    pin = '';
    step = { kind: 'pin', device };
  }

  async function send(device: LanDevice) {
    if (!target || busy) return;
    busy = device.id;
    error = null;
    try {
      await lanApi.send(device.id, target.url, target.title);
      toasts.success(`Sent to ${device.name}`);
      close();
    } catch (e) {
      error = lanErrorMessage(e, device.name);
      if (isLanError(e) && e.code === 'rejected') askPin(device);
    } finally {
      busy = null;
    }
  }

  async function pairAndSend(e: SubmitEvent) {
    e.preventDefault();
    if (step.kind !== 'pin' || !isPin(pin) || busy) return;
    const device = step.device;
    busy = device.id;
    error = null;
    let paired: LanDevice;
    try {
      paired = await lanApi.pair(device.id, pin);
    } catch (err) {
      error = lanErrorMessage(err, device.name);
      busy = null;
      return;
    }
    busy = null;
    step = { kind: 'list' };
    await lan.refreshDevices();
    await send(paired);
  }

  async function connect(e: SubmitEvent) {
    e.preventDefault();
    if (!address.trim() || busy) return;
    busy = 'address';
    error = null;
    let device: LanDevice;
    try {
      device = await lanApi.addAddress(address.trim());
    } catch (err) {
      error = lanErrorMessage(err, 'that address');
      busy = null;
      return;
    }
    busy = null;
    await lan.refreshDevices();
    if (device.paired) {
      step = { kind: 'list' };
      await send(device);
    } else {
      askPin(device);
    }
  }
</script>

{#snippet row(device: LanDevice, detail: string, onclick: () => void)}
  <li>
    <button
      type="button"
      class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left hover:bg-surface-2 disabled:opacity-60"
      disabled={busy !== null}
      {onclick}
    >
      {#if device.platform === 'android'}
        <Smartphone size={18} class="shrink-0 text-muted" />
      {:else}
        <Monitor size={18} class="shrink-0 text-muted" />
      {/if}
      <span class="min-w-0 flex-1">
        <span class="block truncate font-medium">{device.name}</span>
        <span class="block text-xs text-muted">{detail}</span>
      </span>
      {#if busy === device.id}<LoaderCircle size={16} class="animate-spin text-muted" />{/if}
    </button>
  </li>
{/snippet}

<Modal open={target !== null} title="Send to device" onclose={close}>
  {#if target}
    <p class="mb-3 truncate text-sm text-muted" title={target.title}>{target.title}</p>

    {#if !lan.status?.running}
      <p class="text-sm">Sharing with devices on this network is turned off.</p>
      <button type="button" class="btn-primary mt-3" disabled={lan.busy} onclick={() => lan.setEnabled(true)}>
        Turn on
      </button>
    {:else if step.kind === 'list'}
      {#if sections.paired.length === 0 && sections.others.length === 0}
        <p class="flex items-center gap-2 text-sm text-muted" role="status">
          <LoaderCircle size={16} class="animate-spin" /> Looking for devices on this Wi-Fi…
        </p>
        <p class="hint mt-1">On your phone, open Hashlark and turn on Settings → Devices on this network.</p>
      {/if}
      {#if sections.paired.length}
        <ul class="-mx-3 space-y-0.5">
          {#each sections.paired as device (device.id)}
            {@render row(device, device.online ? 'On this network' : 'Not seen right now; will try where it was last', () => send(device))}
          {/each}
        </ul>
      {/if}
      {#if sections.others.length}
        <h3 class="label mt-3">Pair with</h3>
        <ul class="-mx-3 space-y-0.5">
          {#each sections.others as device (device.id)}
            {@render row(device, 'Select to pair', () => askPin(device))}
          {/each}
        </ul>
      {/if}
      <button type="button" class="btn-ghost mt-3" disabled={busy !== null} onclick={() => ((step = { kind: 'address' }), (error = null))}>
        <Pencil size={15} /> Enter address…
      </button>
    {:else if step.kind === 'pin'}
      <form class="space-y-3" onsubmit={pairAndSend}>
        <p class="text-sm">{pinHint(step.device)}</p>
        <label class="block max-w-48">
          <span class="label">PIN</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="input font-mono tracking-widest"
            inputmode="numeric"
            autocomplete="one-time-code"
            maxlength="6"
            autofocus
            bind:value={pin}
            oninput={() => (pin = pin.replace(/\D/g, '').slice(0, 6))}
          />
        </label>
        <div class="flex gap-2">
          <button type="button" class="btn-ghost" onclick={() => ((step = { kind: 'list' }), (error = null))}>
            <ArrowLeft size={15} /> Back
          </button>
          <button type="submit" class="btn-primary" disabled={!isPin(pin) || busy !== null}>
            {#if busy}<LoaderCircle size={15} class="animate-spin" />{/if} Pair and send
          </button>
        </div>
      </form>
    {:else}
      <form class="space-y-3" onsubmit={connect}>
        <p class="text-sm">Hashlark on the phone shows its address under Settings → Devices on this network.</p>
        <label class="block">
          <span class="label">Address</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input class="input" placeholder="192.168.1.20:47821" autofocus bind:value={address} />
        </label>
        <div class="flex gap-2">
          <button type="button" class="btn-ghost" onclick={() => ((step = { kind: 'list' }), (error = null))}>
            <ArrowLeft size={15} /> Back
          </button>
          <button type="submit" class="btn-primary" disabled={!address.trim() || busy !== null}>
            {#if busy}<LoaderCircle size={15} class="animate-spin" />{/if} Connect
          </button>
        </div>
      </form>
    {/if}

    {#if error}<p class="mt-3 text-sm text-err" role="alert">{error}</p>{/if}
  {/if}
</Modal>
