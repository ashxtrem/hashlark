<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { RefreshCw } from '@lucide/svelte';
  import Modal from './Modal.svelte';

  interface Props {
    /** Torrents to recheck; the dialog is open while this is non-empty. */
    ids: string[];
    onclose: () => void;
    onconfirm: (ids: string[]) => void;
  }

  let { ids, onclose, onconfirm }: Props = $props();
</script>

<Modal open={ids.length > 0} title="Force recheck" {onclose}>
  <p>
    Re-verify all downloaded data of {ids.length === 1 ? '1 torrent' : `${ids.length} torrents`}? Downloading waits until
    the check finishes.
  </p>
  {#snippet actions()}
    <button class="btn-ghost" onclick={onclose}>Cancel</button>
    <button class="btn-primary" onclick={() => onconfirm([...ids])}><RefreshCw size={15} /> Recheck</button>
  {/snippet}
</Modal>
