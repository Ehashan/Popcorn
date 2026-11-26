// The Movies and TV Shows screens: a poster grid with sorting and filters.

import { api } from '../api.js';
import { button, h } from '../dom.js';
import { state } from '../state.js';
import { posterGrid } from './poster-grid.js';

const byTitle = (a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base' });

const SORTS = {
  title: { label: 'Title', compare: byTitle },
  year: { label: 'Year', compare: (a, b) => (b.year ?? 0) - (a.year ?? 0) || byTitle(a, b) },
  rating: {
    label: 'Rating',
    compare: (a, b) => (b.rating ?? -1) - (a.rating ?? -1) || byTitle(a, b),
  },
  added: { label: 'Recently added', compare: (a, b) => b.added_at - a.added_at || byTitle(a, b) },
};

const WATCHED = [
  ['all', 'All'],
  ['unwatched', 'Unwatched'],
  ['watched', 'Watched'],
];

export function libraryView(kind, heading) {
  return {
    load: async () => (await api.listTitles()).filter((t) => t.kind === kind),
    build(data, ctx) {
      let titles = data;
      const prefs = state.gridPrefs[kind];
      const grid = posterGrid(ctx, kind);
      const count = h('span', { class: 'count' });
      const empty = h('div', { class: 'empty' });

      const select = (name, label, onChange) => {
        const el = h('select', {
          dataset: { focusKey: `filter:${name}` },
          onChange: () => {
            prefs[name] = el.value;
            onChange();
          },
        });
        return [el, h('label', { class: 'field-inline' }, h('span', { text: label }), el)];
      };
      const apply = () => {
        document.getElementById('app').scrollTop = 0;
        render(true);
      };
      const [sortSelect, sortField] = select('sort', 'Sort', apply);
      const [genreSelect, genreField] = select('genre', 'Genre', apply);
      const [watchedSelect, watchedField] = select('watched', 'Show', apply);

      const setOptions = (el, options, value) => {
        const key = JSON.stringify(options);
        // Rebuilding the options would close an open menu.
        if (el.dataset.options !== key) {
          el.dataset.options = key;
          el.replaceChildren(...options.map(([v, text]) => h('option', { value: v, text })));
        }
        el.value = value;
      };

      function render(reset) {
        const genres = [...new Set(titles.flatMap((t) => t.genres))].sort();
        if (prefs.genre && !genres.includes(prefs.genre)) genres.push(prefs.genre);
        setOptions(
          sortSelect,
          Object.entries(SORTS).map(([value, sort]) => [value, sort.label]),
          prefs.sort,
        );
        setOptions(
          genreSelect,
          [['', 'All genres'], ...genres.map((g) => [g, g])],
          prefs.genre,
        );
        setOptions(watchedSelect, WATCHED, prefs.watched);

        const shown = titles
          .filter((t) => !prefs.genre || t.genres.includes(prefs.genre))
          .filter((t) => prefs.watched === 'all' || t.watched === (prefs.watched === 'watched'))
          .sort(SORTS[prefs.sort].compare);
        if (reset) grid.reset(shown);
        else grid.set(shown);

        count.textContent =
          shown.length === titles.length
            ? `${titles.length}`
            : `${shown.length} of ${titles.length}`;
        empty.hidden = shown.length > 0;
        empty.replaceChildren();
        if (!titles.length) {
          empty.append(
            h('p', { text: `No ${heading.toLowerCase()} found in your media folder yet.` }),
          );
        } else if (!shown.length) {
          empty.append(
            h('p', { text: 'Nothing matches these filters.' }),
            button('Clear filters', {
              onClick: () => {
                Object.assign(prefs, { genre: '', watched: 'all' });
                render(true);
              },
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
          h('h1', {}, heading, ' ', count),
          h('div', { class: 'toolbar' }, sortField, genreField, watchedField),
        ),
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
