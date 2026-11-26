// What the movie and show detail screens share.

import { api } from '../api.js';
import { artwork, button, formatRating, h } from '../dom.js';
import { openFixMatch } from './fix-match.js';
import { row } from './row.js';
import { attempt } from './toast.js';

export function detailsHeader(title, { facts, actions }) {
  const meta = [title.year, ...facts, formatRating(title.rating)].filter(Boolean);
  return h(
    'header',
    { class: 'details-header' },
    h(
      'div',
      { class: 'details-backdrop' },
      artwork(title.backdrop_path, title.title, { className: 'backdrop-art', caption: false }),
      h('div', { class: 'backdrop-fade' }),
    ),
    h(
      'div',
      { class: 'details-main' },
      artwork(title.poster_path, title.title, { className: 'poster details-poster' }),
      h(
        'div',
        { class: 'details-info' },
        h('h1', { text: title.title }),
        h(
          'p',
          { class: 'meta' },
          meta.map((text) => h('span', { text: String(text) })),
          title.certification && h('span', { class: 'badge', text: title.certification }),
        ),
        title.genres.length > 0 &&
          h(
            'ul',
            { class: 'chips' },
            title.genres.map((genre) => h('li', { text: genre })),
          ),
        title.tagline && h('p', { class: 'tagline', text: title.tagline }),
        title.overview && h('p', { class: 'overview', text: title.overview }),
        title.directors.length > 0 &&
          h(
            'p',
            { class: 'credits' },
            h('span', {
              class: 'muted',
              text:
                title.kind === 'show'
                  ? 'Created by '
                  : `Director${title.directors.length > 1 ? 's' : ''}: `,
            }),
            title.directors.join(', '),
          ),
        !title.matched &&
          h('p', {
            class: 'muted',
            text: 'No online details yet. Add a TMDB key in Settings, or use Fix match.',
          }),
        h('div', { class: 'actions' }, actions),
      ),
    ),
  );
}

/** Mark watched, Fix match and Refresh metadata. */
export function titleActions(title, ctx) {
  const watched = button(title.watched ? 'Mark unwatched' : 'Mark watched', {
    icon: 'check',
    dataset: { focusKey: 'details:watched' },
    onClick: async () => {
      watched.disabled = true;
      await attempt(() => api.setTitleWatched(title.id, !title.watched));
      watched.disabled = false;
      ctx.refresh();
    },
  });
  const fix = button('Fix match', {
    dataset: { focusKey: 'details:fix' },
    onClick: () => openFixMatch(title, ctx.refresh),
  });
  const refresh = button('Refresh metadata', {
    dataset: { focusKey: 'details:refresh' },
    onClick: async () => {
      refresh.disabled = true;
      const worked = await attempt(() => api.refreshTitle(title.id), 'Metadata refreshed.');
      refresh.disabled = false;
      if (worked) ctx.refresh();
    },
  });
  return [watched, fix, title.matched && refresh];
}

export function castRow(cast) {
  if (!cast.length) return null;
  return row({
    title: 'Cast',
    key: 'cast',
    items: cast.map((member) =>
      h(
        'figure',
        { class: 'cast-card' },
        artwork(member.profile_path, initials(member.name), { className: 'profile' }),
        h(
          'figcaption',
          {},
          h('span', { class: 'card-title', text: member.name }),
          h('span', { class: 'card-sub', text: member.character ?? '' }),
        ),
      ),
    ),
  });
}

function initials(name) {
  return name
    .split(/\s+/)
    .slice(0, 2)
    .map((word) => [...word][0] ?? '')
    .join('')
    .toUpperCase();
}
