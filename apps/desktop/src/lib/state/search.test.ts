// SPDX-License-Identifier: GPL-3.0-or-later
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Api } from '$lib/api/client';
import { search } from './search.svelte';

const api = new Api({ baseUrl: 'http://127.0.0.1:1/api/v1', token: 'tok' });

/** Stubs `fetch` with an empty search stream; returns the query strings asked for. */
function searches(): URLSearchParams[] {
  const asked: URLSearchParams[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string) => {
      asked.push(new URL(url).searchParams);
      return new Response('', { headers: { 'content-type': 'text/event-stream' } });
    }),
  );
  return asked;
}

afterEach(() => vi.unstubAllGlobals());

describe('a search from a link', () => {
  it('sends the text, categories and IMDb id', async () => {
    const asked = searches();
    search.open({ q: 'the matrix', categories: ['movies', 'tv'], imdb: 'tt0133093' });
    await search.run(api);

    expect(search.shownQuery).toBe('the matrix');
    expect(asked[0]?.get('q')).toBe('the matrix');
    expect(asked[0]?.get('cat')).toBe('movies,tv');
    expect(asked[0]?.get('imdb')).toBe('tt0133093');
  });

  it('drops the IMDb id once the text changes', async () => {
    const asked = searches();
    search.open({ q: 'the matrix', categories: [], imdb: 'tt0133093' });
    await search.run(api);
    search.text = 'dune';
    await search.run(api);
    search.text = 'the matrix';
    await search.run(api);

    expect(asked.map((qs) => qs.get('imdb'))).toEqual(['tt0133093', null, null]);
  });

  it('sends no IMDb id for a link without one', async () => {
    const asked = searches();
    search.open({ q: 'the matrix', categories: [], imdb: 'tt0133093' });
    search.open({ q: 'arch', categories: [] });
    await search.run(api);

    expect(asked[0]?.has('imdb')).toBe(false);
    expect(asked[0]?.has('cat')).toBe(false);
  });
});
