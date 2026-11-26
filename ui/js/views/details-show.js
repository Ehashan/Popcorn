import { api } from '../api.js';
import {
  artwork,
  button,
  episodeCode,
  formatSize,
  formatTime,
  h,
  icon,
  iconButton,
  progressBar,
  progressRatio,
  resumeAt,
} from '../dom.js';
import {
  castSection,
  detailsPage,
  fact,
  revealButton,
  titleActions,
} from '../components/details-parts.js';
import { attempt } from '../components/toast.js';

const seasonName = (season) =>
  season.name || (season.number === 0 ? 'Specials' : `Season ${season.number}`);

const plural = (count, noun) => `${count} ${noun}${count === 1 ? '' : 's'}`;

function playActions(show, ctx) {
  const { play } = show;
  if (!play) return [];
  const code = episodeCode(play.season, play.episode);
  const resume = play.resume_position;
  const episodes = show.seasons.flatMap((s) => s.episodes);
  const started = !show.watched && episodes.some((e) => e.watched || e.position > 0);
  let label = `${started ? 'Play next' : 'Play'} ${code}`;
  if (resume) label = `Resume ${code} from ${formatTime(resume)}`;
  return [
    button(label, {
      icon: 'play',
      variant: 'btn-play',
      dataset: { focusKey: 'details:play', autofocus: '' },
      onClick: () => ctx.play(play.file_id, resume),
    }),
    resume &&
      button('Start over', {
        icon: 'restart',
        dataset: { focusKey: 'details:start-over' },
        onClick: () => ctx.play(play.file_id, null),
      }),
  ];
}

function episodeRow(show, episode, ctx) {
  const resume = resumeAt(episode.position, episode.duration);
  const name = episode.name || `Episode ${episode.episode}`;
  const code = episodeCode(episode.season, episode.episode);
  const facts = [
    resume && `Resume from ${formatTime(resume)}`,
    episode.duration && `${Math.round(episode.duration / 60)} min`,
    formatSize(episode.size),
  ]
    .filter(Boolean)
    .join(' · ');
  const tool = (iconName, label, key, onClick) =>
    iconButton(iconName, label, {
      class: 'ep-tool',
      dataset: { focusKey: `${key}:${episode.file_id}` },
      onClick,
    });

  const watched = h(
    'button',
    {
      type: 'button',
      class: 'icon-btn ep-check',
      title: episode.watched ? `Mark ${name} unwatched` : `Mark ${name} watched`,
      'aria-label': `${name} watched`,
      'aria-pressed': String(episode.watched),
      dataset: { focusKey: `episode-watched:${episode.file_id}` },
      onClick: async () => {
        watched.disabled = true;
        await attempt(() => api.setWatched(episode.file_id, !episode.watched));
        watched.disabled = false;
        ctx.refresh();
      },
    },
    h('span', {}, icon('check')),
  );

  return h(
    'li',
    { class: 'ep' },
    h(
      'button',
      {
        type: 'button',
        class: 'ep-main',
        title: episode.filename,
        'aria-label': `Play ${code}, ${name}`,
        dataset: { focusKey: `episode:${episode.file_id}`, navItem: '' },
        onClick: () => ctx.play(episode.file_id, resume),
      },
      h(
        'span',
        { class: 'ep-still' },
        // Without a still, each episode still gets artwork of its own.
        artwork(episode.still_path, code, {
          caption: false,
          wide: true,
          seed: `${show.title} ${code}`,
        }),
        h('span', { class: 'ep-num', text: String(episode.episode) }),
        h('span', { class: 'play-hover' }, icon('play')),
        !episode.watched &&
          episode.position > 0 &&
          progressBar(progressRatio(episode.position, episode.duration)),
      ),
      h(
        'span',
        { class: 'ep-text' },
        h('span', { class: 'ep-name', text: name }),
        episode.overview && h('span', { class: 'ep-overview', text: episode.overview }),
        h('span', { class: 'ep-facts', text: facts }),
      ),
    ),
    h(
      'div',
      { class: 'ep-end' },
      resume &&
        tool('restart', `Start ${name} over`, 'episode-restart', () =>
          ctx.play(episode.file_id, null),
        ),
      tool('folder', `Show ${name} in Finder`, 'episode-reveal', () =>
        attempt(() => api.revealFile(episode.file_id)),
      ),
      watched,
    ),
  );
}

export default {
  load: (params) => api.getTitle(params.id),
  build(show, ctx) {
    const numbers = show.seasons.map((s) => s.number);
    if (!numbers.includes(ctx.ui.season)) {
      const target = show.play?.season;
      ctx.ui.season = numbers.includes(target) ? target : numbers[0];
    }
    const list = h('ul', { class: 'episodes', dataset: { navGroup: 'list' } });
    const tabs = show.seasons.map((season) =>
      h('button', {
        type: 'button',
        text: seasonName(season),
        dataset: { focusKey: `season:${season.number}` },
        onClick: () => select(season.number),
      }),
    );
    function select(number) {
      ctx.ui.season = number;
      tabs.forEach((tab, i) => tab.setAttribute('aria-pressed', String(numbers[i] === number)));
      const season = show.seasons.find((s) => s.number === number);
      list.replaceChildren(...(season?.episodes ?? []).map((e) => episodeRow(show, e, ctx)));
    }
    select(ctx.ui.season);

    const episodes = show.seasons.flatMap((s) => s.episodes);
    const seasons = plural(show.seasons.length, 'season');
    const bytes = episodes.reduce((total, e) => total + e.size, 0);
    return {
      el: detailsPage(show, ctx, {
        facts: [seasons],
        actions: [
          playActions(show, ctx),
          titleActions(show, ctx),
          show.play && revealButton(show.play.file_id, ctx),
        ],
        extraCredit: ['Episodes', `${episodes.length} on disk`],
        sections: [
          h(
            'div',
            { class: 'seasons' },
            h('h2', { text: 'Episodes' }),
            h('div', { class: 'seg', role: 'group', 'aria-label': 'Season' }, tabs),
          ),
          list,
          castSection(show.cast),
          h('h2', { class: 'section-title', text: 'Files' }),
          h(
            'dl',
            { class: 'file-info' },
            fact('Seasons', show.seasons.length),
            fact('Episodes', episodes.length),
            fact('Size on disk', formatSize(bytes)),
          ),
        ],
      }),
    };
  },
};
