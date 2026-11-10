<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { ChevronDown, ChevronRight, File, Folder } from '@lucide/svelte';
  import FileTree from './FileTree.svelte';
  import { bytes } from '$lib/format';
  import { nodeCheck, nodeIndices, percent, type FileNode } from '$lib/torrents';

  interface Props {
    nodes: FileNode[];
    selected: Set<number>;
    /** Tick or untick these file indices. */
    onchange: (indices: number[], checked: boolean) => void;
    /** Show how much of each file is downloaded. */
    progress?: boolean;
    disabled?: boolean;
    depth?: number;
  }

  let { nodes, selected, onchange, progress = false, disabled = false, depth = 0 }: Props = $props();
  let collapsed = $state<Record<string, boolean>>({});
</script>

<ul class="space-y-0.5" role={depth === 0 ? 'tree' : 'group'}>
  {#each nodes as node (node.path)}
    {@const check = nodeCheck(node, selected)}
    {@const open = !collapsed[node.path]}
    <li role="treeitem" aria-selected={check === 'all'} aria-expanded={node.file ? undefined : open}>
      <div class="flex items-center gap-1.5 rounded-md py-1 pr-1 hover:bg-surface-2" style="padding-left: {depth * 1.1}rem">
        {#if node.file}
          <span class="inline-block w-5"></span>
        {:else}
          <button
            type="button"
            class="inline-flex size-5 items-center justify-center text-muted"
            aria-label={open ? `Collapse ${node.name}` : `Expand ${node.name}`}
            onclick={() => (collapsed[node.path] = open)}
          >
            {#if open}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
          </button>
        {/if}
        <input
          type="checkbox"
          class="size-4 shrink-0 accent-[var(--hl-accent)]"
          aria-label="Download {node.name}"
          checked={check === 'all'}
          {disabled}
          {@attach (el: HTMLInputElement) => {
            el.indeterminate = check === 'some';
          }}
          onchange={(e) => onchange(nodeIndices(node), e.currentTarget.checked)}
        />
        {#if node.file}
          <File size={14} class="shrink-0 text-muted" />
        {:else}
          <Folder size={14} class="shrink-0 text-muted" />
        {/if}
        <span class="min-w-0 flex-1 truncate" title={node.path}>{node.name}</span>
        {#if progress}
          <span class="w-24 shrink-0">
            <span class="block h-1.5 overflow-hidden rounded-full bg-surface-2">
              <span
                class="block h-full rounded-full bg-accent"
                style="width: {node.size ? Math.min(100, (node.have / node.size) * 100) : 0}%"
              ></span>
            </span>
          </span>
          <span class="w-12 shrink-0 text-right text-xs text-muted tabular-nums">
            {percent(node.size ? node.have / node.size : 0)}
          </span>
        {/if}
        <span class="w-20 shrink-0 text-right text-xs text-muted tabular-nums">{bytes(node.size)}</span>
      </div>
      {#if !node.file && open}
        <FileTree nodes={node.children} {selected} {onchange} {progress} {disabled} depth={depth + 1} />
      {/if}
    </li>
  {/each}
</ul>
