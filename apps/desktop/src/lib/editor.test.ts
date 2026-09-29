// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { parse } from 'yaml';
import { applyPick, applyUrl, currentRows, templatizeUrl } from './editor';

const START = `# my notes
schema: 1
id: site
name: Site
links: [https://example.org/]
search:
  path: /search
  response: html
  rows: tr
  fields:
    title: { selector: a }
`;

describe('applyPick', () => {
  it('sets rows and fields and keeps comments', () => {
    let y = applyPick(START, 'rows', 'table.list > tbody > tr');
    y = applyPick(y, 'magnet', "a[href^='magnet:']");
    y = applyPick(y, 'size', 'td.size');
    expect(y).toContain('# my notes');
    const d = parse(y);
    expect(d.search.rows).toBe('table.list > tbody > tr');
    expect(d.search.fields.magnet).toEqual({ selector: "a[href^='magnet:']", attr: 'href', optional: true });
    expect(d.search.fields.size).toEqual({ selector: 'td.size' });
    expect(d.search.fields.title).toEqual({ selector: 'a' });
    expect(currentRows(y)).toBe('table.list > tbody > tr');
  });
});

describe('templatizeUrl', () => {
  it('replaces the search words in paths and params', () => {
    expect(templatizeUrl('https://site.test/search/ubuntu%2024/1/', 'ubuntu 24')).toEqual({
      link: 'https://site.test/',
      path: 'search/{{ query.text | urlencode }}/1/',
      params: {},
    });
    expect(templatizeUrl('https://site.test/index.php?page=torrents&search=Debian', 'debian')).toEqual({
      link: 'https://site.test/',
      path: 'index.php',
      params: { page: 'torrents', search: '{{ query.text }}' },
    });
    expect(templatizeUrl('not a url', 'x')).toBeNull();
  });

  it('writes links, path, params and test query', () => {
    const d = parse(applyUrl(START, 'https://other.test/find?q=linux', 'linux'));
    expect(d.links).toEqual(['https://other.test/']);
    expect(d.search.path).toBe('find');
    expect(d.search.params).toEqual({ q: '{{ query.text }}' });
    expect(d.test.query).toBe('linux');
  });
});
