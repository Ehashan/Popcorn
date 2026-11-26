import { api } from '../api.js';
import {
  artwork,
  button,
  episodeCode,
  formatRating,
  formatTime,
  h,
  icon,
  iconButton,
  progressBar,
  progressRatio,
} from '../dom.js';
import { posterCard } from '../components/poster-card.js';
import { row } from '../components/row.js';

const ROW_LIMIT = 30;
const GENRE_ROWS = 4;
const GENRE_MINIMUM = 4;

const newestFirst = (a, b) => b.added_at - a.added_at;

/** One of the newest titles with a backdrop; the same one until it goes away. */
function pickHero(titles, ui) {
  const kept = titles.find((t) => t.id === ui.heroId);
  if (kept) return kept;
  const recent = [...titles].sort(newestFirst);
  const pool = recent.filter((t) => t.backdrop_path).slice(0, 12);
  const choices = pool.length ? pool : recent.slice(0, 1);
  const hero = choices[Math.floor(Math.random() * choices.length)];
  ui.heroId = hero?.id;
  return hero;
}

function hero(title, details, ctx) {
  const play = details?.play;
  const meta = [title.year, formatRating(title.rating), title.genres.slice(0, 3).join(', ')]
    .filter(Boolean)
    .join('  ·  ');
  const episode = play?.season == null ? '' : ` ${episodeCode(play.season, play.episode)}`;
  return h(
    'section',
    { class: 'hero' },
    artwork(title.backdrop_path, title.title, { className: 'hero-art', caption: false }),
    h('div', { class: 'hero-fade' }),
    h(
      'div',
      { class: 'hero-content' },
      h('h1', { class: 'hero-title', text: title.title }),
      meta && h('p', { class: 'meta', text: meta }),
      title.overview && h('p', { class: 'hero-overview', text: title.overview }),
      h(
        'div',
        { class: 'actions' },
        play &&
          button(
            play.resume_position
              ? `Resume${episode} from ${formatTime(play.resume_position)}`
              : `Play${episode}`,
            {
              icon: 'play',
              variant: 'btn-primary',
              dataset: { focusKey: 'hero:play', autofocus: '' },
              onClick: () => ctx.play(play.file_id, play.resume_position),
            },
          ),
        button('More info', {
          icon: 'info',
          dataset: { focusKey: 'hero:info' },
          onClick: () => ctx.openTitle(title),
        }),
      ),
    ),
  );
}

function continueCard(item, ctx) {
  const parts = [];
  if (item.season != null) {
    parts.push(episodeCode(item.season, item.episode));
    if (item.episode_name) parts.push(item.episode_name);
  } else if (item.duration && item.position) {
    parts.push(`${Math.max(1, Math.round((item.duration - item.position) / 60))} min left`);
  }
  const start = item.position > 0 ? item.position : null;
  return h(
    'div',
    { class: 'continue-card' },
    h(
      'button',
      {
        type: 'button',
        class: 'card',
        'aria-label': `${start ? 'Resume' : 'Play'} ${item.title} ${parts.join(' ')}`,
        dataset: { focusKey: `continue:${item.title_id}`, navItem: '' },
        onClick: () => ctx.play(item.file_id, start),
      },
      h(
        'div',
        { class: 'card-media' },
        artwork(item.still_path ?? item.backdrop_path, item.title, { className: 'wide' }),
        h('span', { class: 'play-overlay' }, icon('play')),
        item.position > 0 &&
          progressBar(progressRatio(item.position, item.duration), 'card-progress'),
      ),
      h('span', { class: 'card-title', text: item.title }),
      h('span', { class: 'card-sub', text: parts.join(' · ') || 'Up next' }),
    ),
    iconButton('info', `Details for ${item.title}`, {
      class: 'continue-info',
      dataset: { focusKey: `continue-info:${item.title_id}` },
      onClick: () => ctx.openTitle({ id: item.title_id, kind: item.kind }),
    }),
  );
}

function genreRows(titles, ctx) {
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
    .map(([genre, list]) =>
      row({
        title: genre,
        key: `genre-${genre.replace(/\W+/g, '-')}`,
        items: [...list]
          .sort((a, b) => (b.rating ?? 0) - (a.rating ?? 0) || newestFirst(a, b))
          .slice(0, ROW_LIMIT)
          .map((t) => posterCard(t, ctx, `genre-${genre.replace(/\W+/g, '-')}`)),
      }),
    );
}

export default {
  async load(params, ui) {
    const [titles, continuing] = await Promise.all([api.listTitles(), api.continueWatching()]);
    const featured = titles.length ? pickHero(titles, ui) : null;
    // Only needed for the hero's Play button, which is optional.
    const details = featured ? await api.getTitle(featured.id).catch(() => null) : null;
    return { titles, continuing, featured, details };
  },
  build({ titles, continuing, featured, details }, ctx) {
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
              variant: 'btn-primary',
              dataset: { focusKey: 'empty:settings' },
              onClick: () => ctx.navigate('settings'),
            }),
          ),
        ),
      };
    }
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
        { class: 'home' },
        featured && hero(featured, details, ctx),
        h(
          'div',
          { class: 'rows' },
          continuing.length > 0 &&
            row({
              title: 'Continue Watching',
              key: 'continue',
              items: continuing.map((item) => continueCard(item, ctx)),
            }),
          row({
            title: 'Recently Added',
            key: 'recent',
            items: posters([...titles].sort(newestFirst), 'recent'),
          }),
          kindRow('movie', 'Movies', 'movies'),
          kindRow('show', 'TV Shows', 'shows'),
          genreRows(titles, ctx),
        ),
      ),
    };
  },
};
