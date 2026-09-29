<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { page } from '$app/state';
  import { History, Plug, Search, Settings, Star } from '@lucide/svelte';
  import { app } from '$lib/state/app.svelte';

  const links = [
    { href: '/', label: 'Search', icon: Search },
    { href: '/favorites', label: 'Favourites', icon: Star },
    { href: '/providers', label: 'Providers', icon: Plug },
    { href: '/history', label: 'History', icon: History },
    { href: '/settings', label: 'Settings', icon: Settings },
  ];

  function active(href: string): boolean {
    return href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href);
  }
</script>

<nav class="flex h-full w-52 shrink-0 flex-col border-r border-line bg-surface px-3 py-4" aria-label="Main">
  <a href="/" class="mb-6 flex items-center gap-2.5 px-2">
    <img src="/favicon.svg" alt="" class="size-8" />
    <span class="text-lg font-semibold tracking-tight">Hashlark</span>
  </a>
  <ul class="flex flex-col gap-1">
    {#each links as link (link.href)}
      <li>
        <a
          href={link.href}
          aria-current={active(link.href) ? 'page' : undefined}
          class="flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors {active(link.href)
            ? 'bg-accent-soft text-accent'
            : 'text-muted hover:bg-surface-2 hover:text-fg'}"
        >
          <link.icon size={18} />
          {link.label}
        </a>
      </li>
    {/each}
  </ul>
  <p class="mt-auto px-2 text-xs text-muted">
    {#if app.version}v{app.version}{/if}
  </p>
</nav>
