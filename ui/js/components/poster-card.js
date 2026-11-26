import { artwork, h, icon } from '../dom.js';

/** A poster in a row or grid. `keyPrefix` tells apart copies of one title. */
export function posterCard(title, ctx, keyPrefix = 'grid') {
  const details = [title.year, title.kind === 'show' ? 'TV' : null].filter(Boolean).join(' · ');
  return h(
    'button',
    {
      type: 'button',
      class: 'card poster-card',
      dataset: { focusKey: `${keyPrefix}:${title.id}`, navItem: '' },
      title: title.title,
      onClick: () => ctx.openTitle(title),
    },
    h(
      'div',
      { class: 'card-media' },
      artwork(title.poster_path, title.title, { className: 'poster' }),
      title.watched &&
        h('span', { class: 'watched-badge', role: 'img', 'aria-label': 'Watched' }, icon('check')),
    ),
    h('span', { class: 'card-title', text: title.title }),
    h('span', { class: 'card-sub', text: details }),
  );
}
