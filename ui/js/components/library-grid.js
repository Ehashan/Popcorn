// The Movies and TV Shows screens: a poster grid with sorting and filters.

import { api } from '../api.js';
import { button, h } from '../dom.js';
import { state } from '../state.js';
import { rememberLibraryStart } from './poster-card.js';
import { posterGrid } from './poster-grid.js';

const byTitle = (a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base' });

const SORTS = {
  added: { label: 'Recently added', compare: (a, b) => b.added_at - a.added_at || byTitle(a, b) },
  title: { label: 'Title', compare: byTitle },
  year: { label: 'Newest first', compare: (a, b) => (b.year ?? 0) - (a.year ?? 0) || byTitle(a, b) },
  rating: {
    label: 'Highest rated',
    compare: (a, b) => (b.rating ?? -1) - (a.rating ?? -1) || byTitle(a, b),
  },
};

const WATCHED = [
  ['all', 'All'],
  ['unwatched', 'Unwatched'],
  ['watched', 'Watched'],
];

export function libraryView(kind, heading) {
  const noun = kind === 'movie' ? 'movie' : 'show';
  const plural = (count) => `${count} ${noun}${count === 1 ? '' : 's'}`;
  return {
    load: async () => {
      const titles = await api.listTitles();
      rememberLibraryStart(titles);
      return titles.filter((t) => t.kind === kind);
    },
    build(data, ctx) {
      let titles = data;
      const prefs = state.gridPrefs[kind];
      const grid = posterGrid(ctx, kind);
      const count = h('p', { class: 'page-count', 'aria-live': 'polite' });
      const empty = h('div', { class: 'empty' });

      const apply = (changes) => {
        Object.assign(prefs, changes);
        document.getElementById('app').scrollTop = 0;
        render(true);
      };
      const toggle = (className, value, label, focus, onClick) =>
        h('button', {
          type: 'button',
          class: className,
          text: label,
          dataset: { value, focusKey: `filter:${focus}:${value}` },
          onClick,
        });

      const watched = h(
        'div',
        { class: 'seg', role: 'group', 'aria-label': 'Filter by watched' },
        WATCHED.map(([value, label]) =>
          toggle('', value, label, 'watched', () => apply({ watched: value })),
        ),
      );
      const sort = h(
        'select',
        {
          class: 'select',
          'aria-label': 'Sort by',
          dataset: { focusKey: 'filter:sort' },
          onChange: () => apply({ sort: sort.value }),
        },
        Object.entries(SORTS).map(([value, { label }]) => h('option', { value, text: label })),
      );
      const genres = h('div', { class: 'chips', role: 'group', 'aria-label': 'Genre' });
      let genreList = null;

      function renderGenres() {
        const names = [...new Set(titles.flatMap((t) => t.genres))].sort();
        if (prefs.genre && !names.includes(prefs.genre)) names.push(prefs.genre);
        const key = JSON.stringify(names);
        // Rebuilding the chips would drop the focus.
        if (key !== genreList) {
          genreList = key;
          const chip = (value, label) =>
            toggle('chip', value, label, 'genre', () => apply({ genre: value }));
          genres.replaceChildren(
            ...(names.length ? [chip('', 'All'), ...names.map((g) => chip(g, g))] : []),
          );
        }
      }

      const press = (group, value) => {
        for (const el of group.children) {
          el.setAttribute('aria-pressed', String(el.dataset.value === value));
        }
      };

      function render(reset) {
        renderGenres();
        press(genres, prefs.genre);
        press(watched, prefs.watched);
        sort.value = prefs.sort;

        const shown = titles
          .filter((t) => !prefs.genre || t.genres.includes(prefs.genre))
          .filter((t) => prefs.watched === 'all' || t.watched === (prefs.watched === 'watched'))
          .sort(SORTS[prefs.sort].compare);
        if (reset) grid.reset(shown);
        else grid.set(shown);

        count.textContent =
          shown.length === titles.length
            ? `${plural(titles.length)} in your library`
            : `${shown.length} of ${plural(titles.length)}`;
        empty.hidden = shown.length > 0;
        empty.replaceChildren();
        if (!titles.length) {
          empty.append(h('p', { text: `No ${noun}s found in your media folder yet.` }));
        } else if (!shown.length) {
          empty.append(
            h('h2', { text: `No ${noun}s match these filters` }),
            h('p', { text: `Try another genre, or show all ${noun}s.` }),
            button(`Show all ${noun}s`, {
              variant: 'btn-sm',
              onClick: () => apply({ genre: '', watched: 'all' }),
            }),
          );
        }
      }

      const el = h(
        'div',
        { class: 'page' },
        h(
          'div',
          { class: 'page-head' },
          h('div', {}, h('h1', { class: 'page-title', text: heading }), count),
          watched,
        ),
        h('div', { class: 'toolbar' }, h('span', { class: 'select-wrap' }, sort), genres),
        empty,
        grid.el,
      );
      render(false);

      return {
        el,
        update(next) {
          titles = next;
          render(false);
        },
        destroy: grid.destroy,
      };
    },
  };
}
