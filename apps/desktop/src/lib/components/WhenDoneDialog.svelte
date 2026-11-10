<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { tick } from 'svelte';
  import Modal from './Modal.svelte';
  import { torrents } from '$lib/state/torrents.svelte';
  import { countdownText, WHEN_DONE_LABELS } from '$lib/torrents';

  let now = $state(Date.now());
  let cancelButton: HTMLButtonElement | undefined = $state();
  let countdown = $derived(torrents.countdown);
  let left = $derived(countdown ? (countdown.endsAt - now) / 1000 : 0);

  $effect(() => {
    if (!countdown) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 250);
    // Cancel is the default button.
    void tick().then(() => cancelButton?.focus());
    return () => clearInterval(timer);
  });
</script>

<Modal open={countdown !== null} title="All downloads are done" required>
  {#if countdown}
    <p class="text-base font-medium" role="timer">{countdownText(countdown.action, left)}</p>
    <p class="mt-2 text-muted">
      You chose “{WHEN_DONE_LABELS[countdown.action]}” for when all downloads are done. Cancel to stay as you are.
    </p>
  {/if}
  {#snippet actions()}
    <button bind:this={cancelButton} class="btn-primary" onclick={() => torrents.cancelWhenDone()}>Cancel</button>
  {/snippet}
</Modal>
