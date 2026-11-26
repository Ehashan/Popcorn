import { artwork, formatRating, formatRuntime, h, icon } from '../dom.js';
import { state } from '../state.js';

const WEEK = 7 * 24 * 60 * 60;
/** Files added this soon after the first ones count as the first import. */
const FIRST_IMPORT = 60 * 60;

/** Recently added, but not just because the whole library is new. */
export function isNew({ added_at: added }) {
  return Date.now() / 1000 - added < WEEK && added - state.libraryStart > FIRST_IMPORT;
}

export function rememberLibraryStart(titles) {
  state.libraryStart = titles.reduce((min, t) => Math.min(min, t.added_at), Infinity);
}

/** A poster in a row or grid. `keyPrefix` tells apart copies of one title. */
export function posterCard(title, ctx, keyPrefix = 'grid') {
  const kind = title.kind === 'show' ? 'Series' : (formatRuntime(title.runtime) ?? 'Movie');
  const rating = formatRating(title.rating);
  return h(
    'button',
    {
      type: 'button',
      class: 'poster',
      dataset: { focusKey: `${keyPrefix}:${title.id}`, navItem: '' },
      title: title.title,
      'aria-label': [title.title, title.year, title.watched && 'watched'].filter(Boolean).join(', '),
      onClick: () => ctx.openTitle(title),
    },
    h(
      'span',
      { class: 'poster-art' },
      artwork(title.poster_path, title.title, { sub: title.year }),
      isNew(title) && h('span', { class: 'badge-new', text: 'New' }),
      title.watched && h('span', { class: 'badge-done' }, icon('check')),
    ),
    h(
      'span',
      { class: 'poster-meta' },
      h('span', { text: kind }),
      h('span', { text: rating ?? String(title.year ?? '') }),
    ),
  );
}
