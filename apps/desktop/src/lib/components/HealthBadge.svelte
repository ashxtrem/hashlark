<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { Schemas } from '$lib/api/client';
  import { dateTime, duration, ERROR_LABELS } from '$lib/format';

  interface Props {
    health: Schemas['HealthSummary'];
  }

  let { health }: Props = $props();

  const LABELS = {
    unknown: 'Not used yet',
    ok: 'Healthy',
    degraded: 'Unreliable',
    failing: 'Failing',
    auto_disabled: 'Paused',
  } as const;

  const STYLES = {
    unknown: 'bg-surface-2 text-muted',
    ok: 'bg-ok-soft text-ok',
    degraded: 'bg-warn-soft text-warn',
    failing: 'bg-err-soft text-err',
    auto_disabled: 'bg-err-soft text-err',
  } as const;

  let detail = $derived.by(() => {
    const parts: string[] = [];
    if (health.success_rate != null) parts.push(`${Math.round(health.success_rate * 100)}% ok`);
    if (health.p50_ms != null) parts.push(`~${duration(health.p50_ms)}`);
    if (health.last_error_kind) parts.push(ERROR_LABELS[health.last_error_kind] ?? health.last_error_kind);
    return parts.join(' · ');
  });

  let title = $derived(
    health.state === 'auto_disabled' && health.disabled_until
      ? `Paused after repeated failures. Retried after ${dateTime(health.disabled_until)}.`
      : health.last_checked_at
        ? `Last used ${dateTime(health.last_checked_at)}`
        : 'Not used yet',
  );
</script>

<span class="inline-flex items-center gap-2 text-xs" {title}>
  <span class="rounded-full px-2 py-0.5 font-medium {STYLES[health.state]}">{LABELS[health.state]}</span>
  {#if detail}<span class="text-muted">{detail}</span>{/if}
</span>
