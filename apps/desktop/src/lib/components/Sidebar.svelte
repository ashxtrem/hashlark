<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { page } from '$app/state';
  import { ArrowDown, ArrowUp, Download, History, Plug, Search, Settings, Star, Turtle } from '@lucide/svelte';
  import { app } from '$lib/state/app.svelte';
  import { torrents } from '$lib/state/torrents.svelte';
  import { totalSpeed } from '$lib/torrents';

  const links = [
    { href: '/', label: 'Search', icon: Search },
    { href: '/downloads', label: 'Downloads', icon: Download },
    { href: '/favorites', label: 'Favourites', icon: Star },
    { href: '/providers', label: 'Providers', icon: Plug },
    { href: '/history', label: 'History', icon: History },
    { href: '/settings', label: 'Settings', icon: Settings },
  ];

  function active(href: string): boolean {
    return href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href);
  }

  let session = $derived(torrents.session);
  let showClient = $derived(torrents.enabled && session !== null);
</script>

<!-- Below 720 px (phones using the web UI) only the icons show. -->
<nav class="flex h-full w-14 shrink-0 flex-col border-r border-line bg-surface px-1.5 py-4 min-[720px]:w-52 min-[720px]:px-3" aria-label="Main">
  <a href="/" class="mb-6 flex items-center gap-2.5 px-1 min-[720px]:px-2">
    <img src="/favicon.svg" alt="" class="size-8" />
    <span class="sr-only text-lg font-semibold tracking-tight min-[720px]:not-sr-only">Hashlark</span>
  </a>
  <ul class="flex flex-col gap-1">
    {#each links as link (link.href)}
      {@const badge = link.href === '/downloads' && torrents.downloading > 0 ? torrents.downloading : 0}
      <li>
        <a
          href={link.href}
          title={link.label}
          aria-current={active(link.href) ? 'page' : undefined}
          class="relative flex items-center justify-center gap-3 rounded-lg px-2 py-2 text-sm font-medium transition-colors min-[720px]:justify-start min-[720px]:px-3 {active(link.href)
            ? 'bg-accent-soft text-accent'
            : 'text-muted hover:bg-surface-2 hover:text-fg'}"
        >
          <link.icon size={18} class="shrink-0" />
          <span class="sr-only min-[720px]:not-sr-only">{link.label}</span>
          {#if badge}
            <span
              class="absolute top-0.5 right-0.5 rounded-full bg-accent px-1.5 text-[0.65rem] leading-4 font-semibold text-accent-fg tabular-nums min-[720px]:static min-[720px]:ml-auto min-[720px]:text-xs"
              aria-label="{badge} downloading"
            >
              {badge}
            </span>
          {/if}
        </a>
      </li>
    {/each}
  </ul>
  <div class="mt-auto space-y-2">
    {#if showClient && session}
      <div class="flex flex-col items-center gap-1 px-1 text-xs text-muted tabular-nums min-[720px]:flex-row min-[720px]:px-2">
        <span class="flex items-center gap-1" title="Download speed">
          <ArrowDown size={13} class="text-accent" /><span class="sr-only">Download</span>
          <span class="hidden min-[720px]:inline">{totalSpeed(session.down_bps)}</span>
        </span>
        <span class="flex items-center gap-1 min-[720px]:ml-2" title="Upload speed">
          <ArrowUp size={13} class="text-ok" /><span class="sr-only">Upload</span>
          <span class="hidden min-[720px]:inline">{totalSpeed(session.up_bps)}</span>
        </span>
        <button
          class="icon-btn size-7 min-[720px]:ml-auto {session.turtle ? 'bg-warn-soft text-warn hover:text-warn' : ''}"
          title={session.turtle ? 'Alternative speed is on' : 'Use alternative speed'}
          aria-label="Alternative speed"
          aria-pressed={session.turtle}
          onclick={() => torrents.setTurtle(!session.turtle)}
        >
          <Turtle size={16} />
        </button>
      </div>
    {/if}
    <p class="hidden px-2 text-xs text-muted min-[720px]:block">
      {#if app.version}v{app.version}{/if}
    </p>
  </div>
</nav>
