// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { relativeSelector, rowSelector, sanitizePage } from './picker';

const PAGE = `
<html><body>
  <div id="main">
    <table class="results">
      <tbody>
        <tr class="row odd"><td class="name"><a class="title" href="/t/1">One</a></td><td class="size">1 GB</td><td><a href="magnet:?xt=urn:btih:1">m</a></td></tr>
        <tr class="row even"><td class="name"><a class="title" href="/t/2">Two</a></td><td class="size">2 GB</td><td><a href="magnet:?xt=urn:btih:2">m</a></td></tr>
        <tr class="row"><td class="name"><a class="title" href="/t/3">Three</a></td><td class="size">3 GB</td><td><a href="magnet:?xt=urn:btih:3">m</a></td></tr>
      </tbody>
    </table>
  </div>
  <script>alert(1)</script>
  <img src="https://tracker.example/pixel.gif" onerror="alert(2)">
</body></html>`;

function doc() {
  return new DOMParser().parseFromString(PAGE, 'text/html');
}

describe('rowSelector', () => {
  it('finds the repeated row and a selector matching all rows', () => {
    const d = doc();
    const title = d.querySelector('a.title')!;
    const found = rowSelector(title)!;
    expect(found.row.tagName).toBe('TR');
    expect(found.selector).toBe('#main > table.results > tbody > tr.row');
    expect(d.querySelectorAll(found.selector)).toHaveLength(3);
  });
});

describe('relativeSelector', () => {
  it('prefers short class-based selectors', () => {
    const d = doc();
    const row = d.querySelectorAll('tr')[1]!;
    expect(relativeSelector(row, row.querySelector('a.title')!)).toBe('a.title');
    expect(relativeSelector(row, row.querySelector('td.size')!)).toBe('td.size');
  });

  it('recognises magnet links and falls back to positions', () => {
    const d = doc();
    const row = d.querySelectorAll('tr')[0]!;
    const magnet = row.querySelector('a[href^="magnet:"]')!;
    expect(relativeSelector(row, magnet)).toBe("a[href^='magnet:']");
    const third = row.querySelectorAll('td')[2]!;
    const sel = relativeSelector(row, third)!;
    expect(row.querySelector(sel)).toBe(third);
    expect(relativeSelector(row, row)).toBeNull();
  });
});

describe('sanitizePage', () => {
  it('removes scripts, handlers and external resources', () => {
    const d = sanitizePage(PAGE);
    expect(d.querySelector('script')).toBeNull();
    const img = d.querySelector('img')!;
    expect(img.getAttribute('src')).toBeNull();
    expect(img.getAttribute('onerror')).toBeNull();
    expect(d.querySelectorAll('tr.row')).toHaveLength(3);
  });
});
