// A poster grid that only builds cards as they are about to scroll into view.

import { h } from '../dom.js';
import { posterCard } from './poster-card.js';

const BATCH = 60;

const signature = (t) => `${t.title}|${t.year}|${t.watched}|${t.poster_path}|${t.kind}`;

/**
 * `ui.rendered` remembers how far the grid was built, so the scroll position
 * can be restored after going back or reloading.
 */
export function posterGrid(ctx, keyPrefix) {
  const grid = h('div', { class: 'grid', dataset: { navGroup: 'grid' } });
  const sentinel = h('div', { class: 'grid-sentinel', 'aria-hidden': 'true' });
  const el = h('div', {}, grid, sentinel);
  // Cards are reused across updates so that images and focus survive.
  let cards = new Map();
  let titles = [];
  let rendered = 0;

  function cardFor(title) {
    const sig = signature(title);
    let entry = cards.get(title.id);
    if (!entry || entry.sig !== sig) {
      entry = { sig, el: posterCard(title, ctx, keyPrefix) };
      cards.set(title.id, entry);
    }
    return entry.el;
  }

  function renderMore(count = BATCH) {
    const end = Math.min(titles.length, rendered + count);
    const fragment = document.createDocumentFragment();
    for (let i = rendered; i < end; i += 1) fragment.append(cardFor(titles[i]));
    grid.append(fragment);
    rendered = end;
    ctx.ui.rendered = rendered;
  }

  const observer = new IntersectionObserver(
    (entries) => {
      if (!entries.some((entry) => entry.isIntersecting) || rendered >= titles.length) return;
      renderMore();
      // Observing again reports the sentinel anew if it is still in range.
      observer.unobserve(sentinel);
      observer.observe(sentinel);
    },
    { root: document.getElementById('app'), rootMargin: '0px 0px 1200px 0px' },
  );

  function set(next) {
    const focused = grid.contains(document.activeElement) ? document.activeElement : null;
    const focusKey = focused?.dataset.focusKey;
    titles = next;
    const target = Math.max(BATCH, ctx.ui.rendered ?? 0);
    const wanted = titles.slice(0, target).map(cardFor);
    // Move as few nodes as possible.
    let cursor = grid.firstChild;
    for (const card of wanted) {
      if (card === cursor) cursor = cursor.nextSibling;
      else grid.insertBefore(card, cursor);
    }
    while (cursor) {
      const stale = cursor;
      cursor = cursor.nextSibling;
      stale.remove();
    }
    rendered = wanted.length;
    const ids = new Set(titles.map((t) => t.id));
    cards = new Map([...cards].filter(([id]) => ids.has(id)));
    if (focusKey && !grid.contains(document.activeElement)) {
      grid.querySelector(`[data-focus-key="${focusKey}"]`)?.focus({ preventScroll: true });
    }
    observer.unobserve(sentinel);
    observer.observe(sentinel);
  }

  /** Starts again from the top, after a sort or filter change. */
  function reset(next) {
    ctx.ui.rendered = 0;
    set(next);
  }

  return { el, set, reset, destroy: () => observer.disconnect() };
}
