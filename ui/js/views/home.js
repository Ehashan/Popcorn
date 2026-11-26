import { api } from '../api.js';
import { button, h } from '../dom.js';
import { hero } from '../components/hero.js';
import { posterCard, rememberLibraryStart } from '../components/poster-card.js';
import { row } from '../components/row.js';
import { wideCard } from '../components/wide-card.js';

const ROW_LIMIT = 30;
const GENRE_ROWS = 4;
const GENRE_MINIMUM = 4;
const FEATURED = 5;
const FEATURE_POOL = 12;

const newestFirst = (a, b) => b.added_at - a.added_at;

/** A few of the newest titles, preferring ones with a backdrop. The same ones
 * stay until they leave the library. */
function pickFeatured(titles, ui) {
  const kept = (ui.featured ?? []).map((id) => titles.find((t) => t.id === id)).filter(Boolean);
  if (kept.length && kept.length === ui.featured.length) return kept;
  const recent = [...titles].sort(newestFirst);
  const withArt = recent.filter((t) => t.backdrop_path).slice(0, FEATURE_POOL);
  const pool = withArt.length ? withArt : recent.slice(0, FEATURE_POOL);
  const chosen = [...pool].sort(() => Math.random() - 0.5).slice(0, FEATURED);
  ui.featured = chosen.map((t) => t.id);
  return chosen;
}

function genreRows(titles, posters) {
  const genres = new Map();
  for (const title of titles) {
    for (const genre of title.genres) {
      if (!genres.has(genre)) genres.set(genre, []);
      genres.get(genre).push(title);
    }
  }
  return [...genres]
    .filter(([, list]) => list.length >= GENRE_MINIMUM)
    .sort((a, b) => b[1].length - a[1].length || a[0].localeCompare(b[0]))
    .slice(0, GENRE_ROWS)
    .map(([genre, list]) => {
      const key = `genre-${genre.replace(/\W+/g, '-')}`;
      const best = [...list].sort((a, b) => (b.rating ?? 0) - (a.rating ?? 0) || newestFirst(a, b));
      return row({ title: genre, key, items: posters(best, key) });
    });
}

export default {
  async load(params, ui) {
    const [titles, continuing] = await Promise.all([api.listTitles(), api.continueWatching()]);
    const featured = await Promise.all(
      pickFeatured(titles, ui).map(async (title) => ({
        title,
        // Only needed for the Play button, which is optional.
        details: await api.getTitle(title.id).catch(() => null),
      })),
    );
    return { titles, continuing, featured };
  },
  build({ titles, continuing, featured }, ctx) {
    if (!titles.length) {
      return {
        el: h(
          'div',
          { class: 'page' },
          h(
            'div',
            { class: 'empty empty-large' },
            h('h1', { text: 'Your library is empty' }),
            h('p', {
              text: 'Add movies or episodes to your media folder and they will appear here.',
            }),
            button('Open Settings', {
              variant: 'btn-play',
              dataset: { focusKey: 'empty:settings' },
              onClick: () => ctx.navigate('settings'),
            }),
          ),
        ),
      };
    }
    rememberLibraryStart(titles);
    const carousel = hero(featured, ctx);
    const posters = (list, key) => list.slice(0, ROW_LIMIT).map((t) => posterCard(t, ctx, key));
    const kindRow = (kind, title, target) => {
      const list = titles.filter((t) => t.kind === kind).sort(newestFirst);
      return (
        list.length > 0 &&
        row({
          title,
          key: target,
          items: posters(list, target),
          onSeeAll: () => ctx.navigate(target),
        })
      );
    };
    return {
      el: h(
        'div',
        {},
        carousel.el,
        h(
          'div',
          { class: 'rows' },
          continuing.length > 0 &&
            row({
              title: 'Continue watching',
              key: 'continue',
              items: continuing.map((item) => wideCard(item, ctx, 'continue')),
            }),
          row({
            title: 'Recently added',
            key: 'recent',
            items: posters([...titles].sort(newestFirst), 'recent'),
          }),
          kindRow('movie', 'Movies', 'movies'),
          kindRow('show', 'TV shows', 'shows'),
          genreRows(titles, posters),
        ),
      ),
      heroStep: carousel.step,
      destroy: carousel.destroy,
    };
  },
};
