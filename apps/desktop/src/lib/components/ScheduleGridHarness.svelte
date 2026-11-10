<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- Test helper: a ScheduleGrid whose value can be read back. -->
<script lang="ts">
  import ScheduleGrid from './ScheduleGrid.svelte';

  let { initial }: { initial: string } = $props();
  // svelte-ignore state_referenced_locally
  let value = $state(initial);
</script>

<ScheduleGrid bind:value />
<output data-testid="value">{value}</output>
