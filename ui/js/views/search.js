// Results for the header search: titles, cast and episode names.

import { api } from '../api.js';
import { h } from '../dom.js';
import { rememberLibraryStart } from '../components/poster-card.js';
import { posterGrid } from '../components/poster-grid.js';
import { toastError } from '../components/toast.js';
import { wideCard } from '../components/wide-card.js';

const plural = (count, noun) => `${count} ${noun}${count === 1 ? '' : 's'}`;

export default {
  load: () => api.listTitles(),
  build(data, ctx) {
    let byId = new Map(data.map((t) => [t.id, t]));
    rememberLibraryStart(data);
    let latest = 0;
    const grid = posterGrid(ctx, 'search');
    grid.el.classList.add('search-grid');
    const heading = h('h1', { class: 'page-title', text: 'Search' });
    const count = h('p', { class: 'page-count', 'aria-live': 'polite' });
    const empty = h('div', { class: 'empty' });
    const episodeList = h('div', { class: 'ep-results', dataset: { navGroup: 'grid' } });
    const episodes = h(
      'section',
      {},
      h('h2', { class: 'results-label', text: 'Episodes' }),
      episodeList,
    );

    async function run(reset) {
      const query = (ctx.ui.query ?? '').trim();
      const request = ++latest;
      let ids = [];
      let hits = [];
      if (query) {
        try {
          [ids, hits] = await Promise.all([api.searchLibrary(query), api.searchEpisodes(query)]);
        } catch (error) {
          toastError(error);
        }
        // A newer keystroke has its own request on the way.
        if (request !== latest) return;
      }
      const results = ids.map((id) => byId.get(id)).filter(Boolean);
      if (reset) grid.reset(results);
      else grid.set(results);
      episodes.hidden = !hits.length;
      episodeList.replaceChildren(...hits.map((hit) => wideCard(hit, ctx, 'search-episode')));

      heading.textContent = query ? `Results for “${query}”` : 'Search';
      const parts = [
        results.length && plural(results.length, 'title'),
        hits.length && plural(hits.length, 'episode'),
      ].filter(Boolean);
      count.textContent = parts.join(' and ');
      empty.hidden = parts.length > 0;
      empty.replaceChildren(
        ...(query
          ? [
              h('h2', { text: `No matches for “${query}”` }),
              h('p', {
                text: 'Check the spelling, or search for a cast member or episode name.',
              }),
            ]
          : [h('p', { text: 'Type a title, an actor or an episode name.' })]),
      );
    }

    return {
      el: h('div', { class: 'page' }, heading, count, empty, grid.el, episodes),
      ready: () => run(false),
      search(query) {
        ctx.ui.query = query;
        return run(true);
      },
      update(next) {
        byId = new Map(next.map((t) => [t.id, t]));
        return run(false);
      },
      destroy: () => {
        latest += 1;
        grid.destroy();
      },
    };
  },
};
