// SPDX-License-Identifier: GPL-3.0-or-later

import { SvelteMap } from 'svelte/reactivity';
import type { Api, Category, ErrorKind, MergedResult, SortOrder } from '$lib/api/client';
import { sortResults, type SortDir } from '$lib/sort';
import { errorMessage } from './toasts.svelte';

export interface ProviderRun {
  id: string;
  state: 'running' | 'ok' | 'failed';
  count?: number;
  latencyMs?: number;
  errorKind?: ErrorKind;
  message?: string;
}

type Status = 'idle' | 'running' | 'done' | 'error';

/** The current search: query, streamed results and per-provider progress. */
class SearchSession {
  text = $state('');
  categories = $state<Category[]>([]);
  sort = $state<SortOrder>('date');
  /** First click on a column is descending; the next click flips it. */
  sortDir = $state<SortDir>('desc');

  status = $state<Status>('idle');
  error = $state<string | null>(null);
  /** Query text of the search whose results are shown. */
  shownQuery = $state('');
  durationMs = $state<number | null>(null);
  results = new SvelteMap<string, MergedResult>();
  runs = new SvelteMap<string, ProviderRun>();

  sorted = $derived(sortResults([...this.results.values()], this.sort, this.sortDir));

  /** Header click: descending the first time, ascending the second. */
  toggleSort(order: SortOrder): void {
    if (this.sort === order) this.sortDir = this.sortDir === 'desc' ? 'asc' : 'desc';
    else {
      this.sort = order;
      this.sortDir = 'desc';
    }
  }

  /** Dropdown: choose a column and start at descending. */
  pickSort(order: SortOrder): void {
    this.sort = order;
    this.sortDir = 'desc';
  }

  private abort: AbortController | null = null;

  toggleCategory(category: Category): void {
    this.categories = this.categories.includes(category)
      ? this.categories.filter((c) => c !== category)
      : [...this.categories, category];
  }

  cancel(): void {
    this.abort?.abort();
    this.abort = null;
    if (this.status === 'running') this.status = 'done';
    for (const run of this.runs.values()) {
      if (run.state === 'running') {
        this.runs.set(run.id, { ...run, state: 'failed', errorKind: 'timeout', message: 'Cancelled' });
      }
    }
  }

  async run(api: Api): Promise<void> {
    const text = this.text.trim();
    if (!text) return;
    this.cancel();
    const abort = new AbortController();
    this.abort = abort;

    this.results.clear();
    this.runs.clear();
    this.status = 'running';
    this.error = null;
    this.durationMs = null;
    this.shownQuery = text;

    try {
      for await (const event of api.search(
        { q: text, categories: this.categories, sort: this.sort },
        abort.signal,
      )) {
        switch (event.event) {
          case 'provider_started':
            this.runs.set(event.provider, { id: event.provider, state: 'running' });
            break;
          case 'results':
            for (const item of event.items) this.results.set(item.id, item);
            break;
          case 'provider_finished':
            this.runs.set(event.provider, {
              id: event.provider,
              state: 'ok',
              count: event.count,
              latencyMs: event.latency_ms,
            });
            break;
          case 'provider_failed':
            this.runs.set(event.provider, {
              id: event.provider,
              state: 'failed',
              errorKind: event.error_kind,
              message: event.message,
              latencyMs: event.latency_ms,
            });
            break;
          case 'done':
            this.durationMs = event.duration_ms;
            break;
        }
      }
      if (this.abort === abort) this.status = 'done';
    } catch (e) {
      if (abort.signal.aborted) return;
      this.status = 'error';
      this.error = errorMessage(e);
    } finally {
      if (this.abort === abort) this.abort = null;
    }
  }
}

export const search = new SearchSession();
