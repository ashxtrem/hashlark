// SPDX-License-Identifier: GPL-3.0-or-later

//! Editing definition YAML from the visual picker, keeping the user's
//! comments and layout.

import { isMap, parseDocument } from 'yaml';

export type PickTarget =
  | 'rows'
  | 'title'
  | 'details'
  | 'magnet'
  | 'download'
  | 'size'
  | 'seeders'
  | 'leechers'
  | 'date';

export const PICK_LABELS: Record<PickTarget, string> = {
  rows: 'Result rows',
  title: 'Title',
  details: 'Details link',
  magnet: 'Magnet link',
  download: '.torrent link',
  size: 'Size',
  seeders: 'Seeders',
  leechers: 'Leechers',
  date: 'Date',
};

const LINK_FIELDS: PickTarget[] = ['details', 'magnet', 'download'];

/** Applies a picked selector to the YAML text. */
export function applyPick(yaml: string, target: PickTarget, selector: string): string {
  const doc = parseDocument(yaml);
  doc.setIn(['search', 'response'], 'html');
  if (target === 'rows') {
    doc.setIn(['search', 'rows'], selector);
  } else {
    const field: Record<string, unknown> = { selector };
    if (LINK_FIELDS.includes(target)) field.attr = 'href';
    if (target === 'magnet' || target === 'date') field.optional = true;
    doc.setIn(['search', 'fields', target], doc.createNode(field, { flow: true }));
  }
  return doc.toString();
}

/**
 * Turns a real search URL into links + path + params templates, replacing
 * the searched words with `query.text`.
 */
export function templatizeUrl(
  href: string,
  term: string,
): { link: string; path: string; params: Record<string, string> } | null {
  let url: URL;
  try {
    url = new URL(href);
  } catch {
    return null;
  }
  const link = `${url.origin}/`;
  const words = term.trim();
  const encodedForms = words
    ? [encodeURIComponent(words), words.replace(/ /g, '+'), words.replace(/ /g, '%20'), words]
    : [];
  let path = url.pathname.replace(/^\//, '');
  for (const form of encodedForms) {
    if (form && path.includes(form)) {
      path = path.replace(form, '{{ query.text | urlencode }}');
      break;
    }
  }
  const params: Record<string, string> = {};
  url.searchParams.forEach((value, key) => {
    params[key] = words && value.trim().toLowerCase() === words.toLowerCase() ? '{{ query.text }}' : value;
  });
  return { link, path: path || '/', params };
}

/** Writes the templatized URL into the YAML. */
export function applyUrl(yaml: string, href: string, term: string): string {
  const parts = templatizeUrl(href, term);
  if (!parts) return yaml;
  const doc = parseDocument(yaml);
  doc.setIn(['links'], doc.createNode([parts.link]));
  doc.setIn(['search', 'path'], parts.path);
  doc.setIn(['search', 'params'], doc.createNode(parts.params));
  if (term.trim()) doc.setIn(['test', 'query'], term.trim());
  return doc.toString();
}

/** The current rows selector in the YAML, if any. */
export function currentRows(yaml: string): string | null {
  try {
    const value = parseDocument(yaml).getIn(['search', 'rows']);
    return typeof value === 'string' ? value : null;
  } catch {
    return null;
  }
}

/** Whether the YAML parses at all (the editor shows a hint otherwise). */
export function parses(yaml: string): boolean {
  try {
    const doc = parseDocument(yaml);
    return doc.errors.length === 0 && isMap(doc.contents);
  } catch {
    return false;
  }
}
