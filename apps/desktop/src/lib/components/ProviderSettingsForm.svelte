<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { untrack } from 'svelte';
  import type { ProviderView } from '$lib/api/client';
  import { app } from '$lib/state/app.svelte';
  import { providers } from '$lib/state/providers.svelte';
  import { errorMessage, toasts } from '$lib/state/toasts.svelte';

  interface Props {
    provider: ProviderView;
  }

  let { provider }: Props = $props();

  // Passwords start empty: an empty password field means "keep the current one".
  let values = $state<Record<string, string>>(
    untrack(() =>
      Object.fromEntries(
        provider.settings.map((s) => [s.name, s.kind === 'password' ? '' : (s.value ?? '')]),
      ),
    ),
  );
  let saving = $state(false);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    const changes: Record<string, string | null> = {};
    for (const s of provider.settings) {
      const v = values[s.name] ?? '';
      if (s.kind === 'password') {
        if (v) changes[s.name] = v;
      } else if (v !== (s.value ?? '')) {
        changes[s.name] = v || null;
      }
    }
    saving = true;
    try {
      providers.put(await app.client.updateProvider(provider.id, { settings: changes }));
      for (const s of provider.settings) if (s.kind === 'password') values[s.name] = '';
      toasts.success('Provider settings saved');
    } catch (err) {
      toasts.error(errorMessage(err));
    } finally {
      saving = false;
    }
  }

  async function clearPassword(name: string) {
    try {
      providers.put(await app.client.updateProvider(provider.id, { settings: { [name]: null } }));
      toasts.success('Removed');
    } catch (err) {
      toasts.error(errorMessage(err));
    }
  }
</script>

<form class="grid gap-3 sm:grid-cols-2" onsubmit={save}>
  {#each provider.settings as s (s.name)}
    <label class="block">
      <span class="label">{s.label}{#if s.required}<span class="text-err"> *</span>{/if}</span>
      {#if s.kind === 'select'}
        <select class="input" bind:value={values[s.name]}>
          <option value="">Default</option>
          {#each Object.entries(s.options) as [value, label] (value)}<option {value}>{label}</option>{/each}
        </select>
      {:else if s.kind === 'checkbox'}
        <select class="input" bind:value={values[s.name]}>
          <option value="">Off</option>
          <option value="true">On</option>
        </select>
      {:else if s.kind === 'password'}
        <input
          class="input"
          type="password"
          autocomplete="new-password"
          placeholder={s.is_set ? 'Saved — leave empty to keep' : ''}
          bind:value={values[s.name]}
        />
        {#if s.is_set}
          <button type="button" class="mt-1 text-xs text-muted underline" onclick={() => clearPassword(s.name)}>Remove saved value</button>
        {/if}
      {:else}
        <input class="input" bind:value={values[s.name]} />
      {/if}
    </label>
  {/each}
  <div class="sm:col-span-2">
    <button class="btn-primary" type="submit" disabled={saving}>Save settings</button>
    <span class="hint ml-2">Passwords are kept in your system keychain.</span>
  </div>
</form>
