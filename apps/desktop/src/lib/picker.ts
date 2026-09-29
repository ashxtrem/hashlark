// SPDX-License-Identifier: GPL-3.0-or-later

//! Building CSS selectors from clicks, for the definition editor.

/** Escapes an identifier for use in a CSS selector. */
export function cssIdent(name: string): string {
  if (typeof CSS !== 'undefined' && CSS.escape) return CSS.escape(name);
  return name.replace(/([^a-zA-Z0-9_-])/g, '\\$1').replace(/^(\d)/, '\\3$1 ');
}

/** Classes worth using in selectors (skips generated/stateful-looking ones). */
function usefulClasses(el: Element): string[] {
  return [...el.classList].filter(
    (c) => c.length < 40 && !/\d{3,}/.test(c) && !/^(active|selected|hover|odd|even|ng-|js-)/.test(c),
  );
}

/** A selector step for one element: tag plus its useful classes. */
function step(el: Element, withClasses = true): string {
  const tag = el.tagName.toLowerCase();
  const classes = withClasses ? usefulClasses(el) : [];
  return tag + classes.map((c) => `.${cssIdent(c)}`).join('');
}

/**
 * A short selector for `el` from the document root: stops at the nearest
 * ancestor with an id, otherwise uses tag+class steps.
 */
export function absoluteSelector(el: Element): string {
  const steps: string[] = [];
  let node: Element | null = el;
  while (node && node.tagName.toLowerCase() !== 'html') {
    if (node.id && !/\d{4,}/.test(node.id)) {
      steps.unshift(`#${cssIdent(node.id)}`);
      break;
    }
    steps.unshift(step(node));
    node = node.parentElement;
  }
  return steps.join(' > ');
}

/**
 * Finds the "row" containing `el`: the nearest ancestor that has at least
 * three siblings of the same kind (a table row, a list item, a card), and
 * returns a selector matching all those siblings.
 */
export function rowSelector(el: Element): { selector: string; row: Element } | null {
  // Every ancestor that repeats at least three times among its siblings.
  const candidates: { node: Element; siblings: Element[] }[] = [];
  let node: Element | null = el;
  while (node?.parentElement) {
    const parent: Element = node.parentElement;
    const tag: string = node.tagName;
    const siblings = [...parent.children].filter((c) => c.tagName === tag);
    if (siblings.length >= 3) candidates.push({ node, siblings });
    node = parent;
  }
  if (candidates.length === 0) return null;
  // Table rows win; otherwise the most repeated ancestor (cells repeat
  // less often than rows), preferring the higher one on ties.
  const best =
    candidates.find((c) => c.node.tagName === 'TR') ??
    candidates.reduce((a, b) => (b.siblings.length >= a.siblings.length ? b : a));
  const shared = usefulClasses(best.node).filter(
    (c) => best.siblings.filter((s) => s.classList.contains(c)).length >= 3,
  );
  const own = best.node.tagName.toLowerCase() + shared.map((c) => `.${cssIdent(c)}`).join('');
  return { selector: `${absoluteSelector(best.node.parentElement!)} > ${own}`, row: best.node };
}

/**
 * The shortest selector, relative to `row`, whose first match inside the
 * row is `el`.
 */
export function relativeSelector(row: Element, el: Element): string | null {
  if (row === el) return null;
  const path: Element[] = [];
  let node: Element | null = el;
  while (node && node !== row) {
    path.unshift(node);
    node = node.parentElement;
  }
  if (node !== row) return null;

  const candidates: string[] = [];
  // Special case: magnet links.
  if (el.tagName === 'A' && el.getAttribute('href')?.startsWith('magnet:')) {
    candidates.push("a[href^='magnet:']");
  }
  // Shortest suffix of the path first; with classes, then with positions.
  for (let start = path.length - 1; start >= 0; start--) {
    const suffix = path.slice(start);
    candidates.push(suffix.map((e) => step(e)).join(' '));
    candidates.push(
      suffix
        .map((e) => {
          const siblings = e.parentElement ? [...e.parentElement.children].filter((s) => s.tagName === e.tagName) : [];
          const base = step(e, false);
          return siblings.length > 1 ? `${base}:nth-of-type(${siblings.indexOf(e) + 1})` : base;
        })
        .join(' > '),
    );
  }
  for (const candidate of candidates) {
    try {
      if (row.querySelector(candidate) === el) return candidate;
    } catch {
      // Not a valid selector in this engine; try the next one.
    }
  }
  return null;
}

/**
 * Makes fetched HTML safe to show for picking: no scripts, frames, event
 * handlers, external resources or navigation.
 */
export function sanitizePage(html: string): Document {
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc
    .querySelectorAll('script, noscript, iframe, frame, object, embed, link, meta, base, form')
    .forEach((n) => {
      if (n.tagName === 'FORM') {
        // Keep the contents of forms (results are sometimes inside one).
        n.replaceWith(...n.childNodes);
      } else {
        n.remove();
      }
    });
  doc.querySelectorAll('*').forEach((el) => {
    for (const attr of [...el.attributes]) {
      const name = attr.name.toLowerCase();
      if (name.startsWith('on') || name === 'srcset' || name === 'src' || name === 'style' && /url\(/i.test(attr.value)) {
        el.removeAttribute(attr.name);
      }
    }
  });
  return doc;
}
