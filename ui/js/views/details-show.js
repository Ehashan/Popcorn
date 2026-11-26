import { api } from '../api.js';
import {
  artwork,
  button,
  episodeCode,
  formatResolution,
  formatSize,
  formatTime,
  h,
  icon,
  iconButton,
  progressBar,
  progressRatio,
  resumeAt,
} from '../dom.js';
import { castRow, detailsHeader, titleActions } from '../components/details-parts.js';
import { attempt } from '../components/toast.js';

const seasonName = (season) =>
  season.name || (season.number === 0 ? 'Specials' : `Season ${season.number}`);

function playActions(show, ctx) {
  const { play } = show;
  if (!play) return [];
  const code = episodeCode(play.season, play.episode);
  const resume = play.resume_position;
  const episodes = show.seasons.flatMap((s) => s.episodes);
  const started = !show.watched && episodes.some((e) => e.watched || e.position > 0);
  let label = `${started ? 'Play next' : 'Play'} · ${code}`;
  if (resume) label = `Resume ${code} from ${formatTime(resume)}`;
  return [
    button(label, {
      icon: 'play',
      variant: 'btn-primary',
      dataset: { focusKey: 'details:play', autofocus: '' },
      onClick: () => ctx.play(play.file_id, resume),
    }),
    resume &&
      button('Start over', {
        dataset: { focusKey: 'details:start-over' },
        onClick: () => ctx.play(play.file_id, null),
      }),
  ];
}

function episodeRow(episode, ctx) {
  const resume = resumeAt(episode.position, episode.duration);
  const name = episode.name || `Episode ${episode.episode}`;
  const facts = [
    resume
      ? `Resume from ${formatTime(resume)}`
      : episode.duration
        ? `${Math.round(episode.duration / 60)} min`
        : null,
    formatSize(episode.size),
    formatResolution(episode.width, episode.height),
  ]
    .filter(Boolean)
    .join(' · ');
  const toggle = iconButton(
    'check',
    episode.watched ? `Mark ${name} unwatched` : `Mark ${name} watched`,
    {
      class: episode.watched ? 'toggle-watched on' : 'toggle-watched',
      'aria-pressed': String(episode.watched),
      dataset: { focusKey: `episode-watched:${episode.file_id}` },
      onClick: async () => {
        toggle.disabled = true;
        await attempt(() => api.setWatched(episode.file_id, !episode.watched));
        toggle.disabled = false;
        ctx.refresh();
      },
    },
  );
  return h(
    'li',
    { class: 'episode' },
    h(
      'button',
      {
        type: 'button',
        class: 'episode-play',
        title: episode.filename,
        dataset: { focusKey: `episode:${episode.file_id}`, navItem: '' },
        onClick: () => ctx.play(episode.file_id, resume),
      },
      h(
        'div',
        { class: 'episode-still' },
        artwork(episode.still_path, episodeCode(episode.season, episode.episode), {
          className: 'wide',
        }),
        h('span', { class: 'play-overlay' }, icon('play')),
        (resume || (episode.position > 0 && !episode.watched)) &&
          progressBar(progressRatio(episode.position, episode.duration), 'card-progress'),
      ),
      h(
        'div',
        { class: 'episode-text' },
        h(
          'span',
          { class: 'episode-name' },
          `${episode.episode}. ${name}`,
          episode.watched &&
            h(
              'span',
              { class: 'episode-tick', role: 'img', 'aria-label': 'Watched' },
              icon('check'),
            ),
        ),
        episode.overview && h('span', { class: 'episode-overview', text: episode.overview }),
        h('span', { class: 'muted', text: facts }),
      ),
    ),
    h(
      'div',
      { class: 'episode-actions' },
      resume &&
        button('Start over', {
          dataset: { focusKey: `episode-restart:${episode.file_id}` },
          onClick: () => ctx.play(episode.file_id, null),
        }),
      toggle,
      button('Show in Finder', {
        variant: 'btn-quiet',
        dataset: { focusKey: `episode-reveal:${episode.file_id}` },
        onClick: () => attempt(() => api.revealFile(episode.file_id)),
      }),
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
    const list = h('ul', { class: 'episode-list', dataset: { navGroup: 'list' } });
    const chips = show.seasons.map((season) =>
      h('button', {
        type: 'button',
        class: 'season-chip',
        text: seasonName(season),
        dataset: { focusKey: `season:${season.number}` },
        onClick: () => select(season.number),
      }),
    );
    function select(number) {
      ctx.ui.season = number;
      chips.forEach((chip, i) =>
        chip.setAttribute('aria-pressed', String(numbers[i] === number)),
      );
      const season = show.seasons.find((s) => s.number === number);
      list.replaceChildren(...(season?.episodes ?? []).map((e) => episodeRow(e, ctx)));
    }
    select(ctx.ui.season);

    const seasonCount = show.seasons.length;
    return {
      el: h(
        'article',
        { class: 'details' },
        detailsHeader(show, {
          facts: [`${seasonCount} season${seasonCount === 1 ? '' : 's'}`],
          actions: [playActions(show, ctx), titleActions(show, ctx)],
        }),
        h(
          'div',
          { class: 'rows' },
          h(
            'section',
            { class: 'section' },
            h('h2', { text: 'Episodes' }),
            h('div', { class: 'season-bar', role: 'group', 'aria-label': 'Season' }, chips),
            list,
          ),
          castRow(show.cast),
        ),
      ),
    };
  },
};
