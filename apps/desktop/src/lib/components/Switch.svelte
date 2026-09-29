<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  interface Props {
    checked: boolean;
    label: string;
    disabled?: boolean;
    onchange?: (checked: boolean) => void;
  }

  let { checked = $bindable(), label, disabled = false, onchange }: Props = $props();
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  class="relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors disabled:opacity-50 {checked
    ? 'bg-accent'
    : 'bg-line'}"
  onclick={() => {
    checked = !checked;
    onchange?.(checked);
  }}
>
  <span
    class="inline-block size-4 rounded-full bg-white shadow transition-transform {checked
      ? 'translate-x-4.5'
      : 'translate-x-0.5'}"
  ></span>
</button>
