// SPDX-License-Identifier: GPL-3.0-or-later

//! `hashlark://search?q=<text>&cat=<categories>&imdb=<id>`: the link another
//! app opens to start a search in Hashlark (see `docs/user-guide.md`).

import type { Category } from '$lib/api/client';
import { CATEGORY_LABELS } from '$lib/format';

/** What a search link asks for. `q` is empty when the link only opens the app. */
export interface SearchLink {
  q: string;
  categories: Category[];
  /** IMDb id (`tt1234567`), for providers that can search by it. */
  imdb?: string;
}

/** Longest search text taken from a link, in characters. */
const MAX_QUERY = 200;
const IMDB_ID = /^tt\d{5,10}$/;
// Matched by hand: webviews don't agree on the host of a `hashlark:` URL.
const LINK = /^hashlark:(?:\/\/)?search\/?(?:\?([^#]*))?(?:#.*)?$/i;

/**
 * Reads a search link. Anything that isn't `hashlark://search` gives `null`.
 * The link is untrusted: only `q` (trimmed, at most 200 characters), known
 * categories in `cat` (comma-separated, or repeated) and a well-formed
 * `imdb` id are kept.
 */
export function parseSearchLink(url: string): SearchLink | null {
  const match = LINK.exec(url.trim());
  if (!match) return null;
  const params = new URLSearchParams(match[1] ?? '');

  const q = [...(params.get('q') ?? '').trim()].slice(0, MAX_QUERY).join('').trim();
  const categories = params
    .getAll('cat')
    .flatMap((value) => value.split(','))
    .map((c) => c.trim().toLowerCase())
    .filter((c): c is Category => Object.hasOwn(CATEGORY_LABELS, c));
  const imdb = params.get('imdb')?.trim() ?? '';

  return {
    q,
    categories: [...new Set(categories)],
    ...(IMDB_ID.test(imdb) ? { imdb } : {}),
  };
}
