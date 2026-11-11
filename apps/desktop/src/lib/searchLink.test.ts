// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { parseSearchLink } from './searchLink';

describe('parseSearchLink', () => {
  it('reads the text, categories and IMDb id', () => {
    expect(parseSearchLink('hashlark://search?q=the+matrix&cat=movies&imdb=tt0133093')).toEqual({
      q: 'the matrix',
      categories: ['movies'],
      imdb: 'tt0133093',
    });
    expect(parseSearchLink('hashlark://search?q=arch')).toEqual({ q: 'arch', categories: [] });
  });

  it('accepts the forms a URL parser may hand over', () => {
    const want = { q: 'arch', categories: [] };
    expect(parseSearchLink('hashlark://search/?q=arch')).toEqual(want);
    expect(parseSearchLink('hashlark:search?q=arch')).toEqual(want);
    expect(parseSearchLink('HASHLARK://Search?q=arch#top')).toEqual(want);
    expect(parseSearchLink('  hashlark://search?q=arch\n')).toEqual(want);
  });

  it('gives an empty text for a link without q', () => {
    expect(parseSearchLink('hashlark://search')).toEqual({ q: '', categories: [] });
    expect(parseSearchLink('hashlark://search?')).toEqual({ q: '', categories: [] });
    expect(parseSearchLink('hashlark://search?q=%20+&cat=tv')).toEqual({ q: '', categories: ['tv'] });
  });

  it('decodes percent-encoding and plus signs', () => {
    expect(parseSearchLink('hashlark://search?q=am%C3%A9lie%20%26%20co+2001')?.q).toBe('amélie & co 2001');
    expect(parseSearchLink('hashlark://search?q=c%2B%2B')?.q).toBe('c++');
    // A broken escape is kept as text, not thrown.
    expect(parseSearchLink('hashlark://search?q=100%')?.q).toBe('100%');
  });

  it('trims the text and cuts it at 200 characters', () => {
    expect(parseSearchLink('hashlark://search?q=++dune++')?.q).toBe('dune');
    expect(parseSearchLink(`hashlark://search?q=${'a'.repeat(500)}`)?.q).toBe('a'.repeat(200));
    // Counted in characters, so an emoji is never cut in half.
    expect(parseSearchLink(`hashlark://search?q=${'a'.repeat(199)}%F0%9F%98%80b`)?.q).toBe(`${'a'.repeat(199)}😀`);
    expect(parseSearchLink(`hashlark://search?q=${'a'.repeat(199)}+b`)?.q).toBe('a'.repeat(199));
  });

  it('keeps known categories, comma-separated or repeated', () => {
    expect(parseSearchLink('hashlark://search?q=x&cat=movies,tv')?.categories).toEqual(['movies', 'tv']);
    expect(parseSearchLink('hashlark://search?q=x&cat=movies%2Ctv')?.categories).toEqual(['movies', 'tv']);
    expect(parseSearchLink('hashlark://search?q=x&cat=movies&cat=tv')?.categories).toEqual(['movies', 'tv']);
    expect(parseSearchLink('hashlark://search?q=x&cat=TV,%20anime,tv')?.categories).toEqual(['tv', 'anime']);
  });

  it('drops unknown categories', () => {
    expect(parseSearchLink('hashlark://search?q=x&cat=movies,xxx,,constructor')?.categories).toEqual(['movies']);
    expect(parseSearchLink('hashlark://search?q=x&cat=films')?.categories).toEqual([]);
  });

  it('drops a malformed IMDb id', () => {
    for (const imdb of ['0133093', 'tt123', 'tt12345678901', 'tt0133093x', 'TT0133093', 'tt0133093%26q%3Dy', '']) {
      expect(parseSearchLink(`hashlark://search?q=x&imdb=${imdb}`)).toEqual({ q: 'x', categories: [] });
    }
    expect(parseSearchLink('hashlark://search?q=x&imdb=tt12345')?.imdb).toBe('tt12345');
  });

  it('ignores every other parameter', () => {
    expect(parseSearchLink('hashlark://search?q=x&providers=a&url=https%3A%2F%2Fexample.org&sort=size')).toEqual({
      q: 'x',
      categories: [],
    });
  });

  it('rejects other schemes, hosts and paths', () => {
    for (const url of [
      'https://search?q=x',
      'magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567',
      'xhashlark://search?q=x',
      'hashlark://repo?url=https%3A%2F%2Fexample.org',
      'hashlark://search/more?q=x',
      'hashlark://searching?q=x',
      'hashlark://user@search?q=x',
      'hashlark://search:80?q=x',
      'hashlark:///search?q=x',
      'hashlark://',
      'search?q=x',
      '',
    ]) {
      expect(parseSearchLink(url), url).toBeNull();
    }
  });
});
