// What the movie and show detail screens share.

import { api } from '../api.js';
import { artwork, button, factList, FACES, h, hashOf, icon } from '../dom.js';
import { state } from '../state.js';
import { openFixMatch } from './fix-match.js';
import { attempt } from './toast.js';

function credit(label, value) {
  return value && h('div', {}, h('dt', { text: label }), h('dd', { text: value }));
}

/** The whole screen: backdrop, poster and header text, then `sections`. */
export function detailsPage(title, ctx, { facts, actions, extraCredit, sections }) {
  const gridKind = title.kind === 'show' ? 'shows' : 'movies';
  const starring = title.cast
    .slice(0, 3)
    .map((member) => member.name)
    .join(', ');
  return h(
    'article',
    { class: 'detail' },
    h(
      'div',
      { class: 'detail-hero' },
      artwork(title.backdrop_path, title.title, { caption: false, wide: true }),
      h('div', { class: 'grain' }),
    ),
    h(
      'div',
      { class: 'detail-body' },
      artwork(title.poster_path, title.title, { className: 'detail-poster', sub: title.year }),
      h(
        'div',
        {},
        h(
          'button',
          { type: 'button', class: 'back', dataset: { focusKey: 'details:back' }, onClick: ctx.back },
          icon('back'),
          'Back',
        ),
        h('h1', { class: 'detail-title', text: title.title }),
        factList(title, ...facts),
        title.genres.length > 0 &&
          h(
            'div',
            { class: 'genres' },
            title.genres.map((genre) =>
              h('button', {
                type: 'button',
                class: 'genre',
                text: genre,
                dataset: { focusKey: `details:genre:${genre}` },
                onClick: () => {
                  state.gridPrefs[title.kind].genre = genre;
                  ctx.navigate(gridKind);
                },
              }),
            ),
          ),
        title.tagline && h('p', { class: 'tagline', text: title.tagline }),
        title.overview && h('p', { class: 'overview', text: title.overview }),
        !title.matched &&
          h('p', {
            class: 'muted',
            text: 'No online details yet. Add a TMDB key in Settings, or use Fix match.',
          }),
        h('div', { class: 'actions' }, actions),
        h(
          'dl',
          { class: 'credits' },
          credit(
            title.kind === 'show' ? 'Created by' : 'Directed by',
            title.directors.join(', '),
          ),
          credit('Starring', starring),
          extraCredit && credit(...extraCredit),
        ),
        sections,
      ),
    ),
  );
}

export function revealButton(fileId, ctx) {
  return button('Show in Finder', {
    icon: 'folder',
    variant: 'btn-quiet',
    dataset: { focusKey: 'details:reveal' },
    onClick: () => attempt(() => api.revealFile(fileId)),
  });
}

/** Mark watched, Fix match and Refresh metadata. */
export function titleActions(title, ctx) {
  const watched = button(title.watched ? 'Mark as unwatched' : 'Mark as watched', {
    icon: title.watched ? 'eye' : 'check',
    dataset: { focusKey: 'details:watched' },
    onClick: async () => {
      watched.disabled = true;
      await attempt(() => api.setTitleWatched(title.id, !title.watched));
      watched.disabled = false;
      ctx.refresh();
    },
  });
  const fix = button('Fix match', {
    icon: 'wand',
    variant: 'btn-quiet',
    dataset: { focusKey: 'details:fix' },
    onClick: () => openFixMatch(title, ctx.refresh),
  });
  const refresh = button('Refresh metadata', {
    icon: 'refresh',
    variant: 'btn-quiet',
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

function face(member) {
  // Initials stay underneath until a photo covers them.
  const initials = member.name
    .split(/\s+/)
    .slice(0, 2)
    .map((word) => [...word][0] ?? '')
    .join('')
    .toUpperCase();
  const el = h('div', {
    class: `face face-p${(hashOf(member.name) % FACES) + 1}`,
    text: initials,
  });
  if (member.profile_path) {
    const photo = artwork(member.profile_path, '', { caption: false });
    const img = photo.querySelector('img');
    if (img) {
      img.addEventListener('error', () => img.remove());
      el.append(img);
    }
  }
  return el;
}

export function castSection(cast) {
  if (!cast.length) return null;
  return [
    h('h2', { class: 'section-title', text: 'Cast' }),
    h(
      'div',
      { class: 'cast', dataset: { scrollKey: 'cast' } },
      cast.map((member) =>
        h(
          'div',
          { class: 'person' },
          face(member),
          h('b', { text: member.name }),
          h('span', { text: member.character ?? '' }),
        ),
      ),
    ),
  ];
}

export function fact(label, value) {
  return value ? h('div', {}, h('dt', { text: label }), h('dd', { text: String(value) })) : null;
}
