import { api } from '../api.js';
import {
  button,
  formatResolution,
  formatRuntime,
  formatSize,
  formatTime,
  h,
  progressBar,
  progressRatio,
  resumeAt,
} from '../dom.js';
import { castRow, detailsHeader, titleActions } from '../components/details-parts.js';
import { attempt } from '../components/toast.js';

function playActions(movie, ctx) {
  const { play } = movie;
  if (!play) return [];
  const resume = play.resume_position;
  return [
    button(resume ? `Resume from ${formatTime(resume)}` : 'Play', {
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

function fileRow(file, ctx, several) {
  const facts = [formatSize(file.size), formatResolution(file.width, file.height)]
    .filter(Boolean)
    .join(' · ');
  return h(
    'li',
    { class: 'file-row' },
    h(
      'div',
      { class: 'file-info' },
      h('span', { class: 'file-name', text: file.filename }),
      h('span', { class: 'muted', text: facts }),
      !file.watched &&
        resumeAt(file.position, file.duration) &&
        progressBar(progressRatio(file.position, file.duration)),
    ),
    several &&
      button('Play', {
        icon: 'play',
        dataset: { focusKey: `file-play:${file.file_id}` },
        onClick: () => ctx.play(file.file_id, resumeAt(file.position, file.duration)),
      }),
    button('Show in Finder', {
      dataset: { focusKey: `file-reveal:${file.file_id}` },
      onClick: () => attempt(() => api.revealFile(file.file_id)),
    }),
  );
}

export default {
  load: (params) => api.getTitle(params.id),
  build(movie, ctx) {
    const several = movie.files.length > 1;
    return {
      el: h(
        'article',
        { class: 'details' },
        detailsHeader(movie, {
          facts: [formatRuntime(movie.runtime)],
          actions: [playActions(movie, ctx), titleActions(movie, ctx)],
        }),
        h(
          'div',
          { class: 'rows' },
          castRow(movie.cast),
          h(
            'section',
            { class: 'section' },
            h('h2', { text: several ? 'Files' : 'File' }),
            h(
              'ul',
              { class: 'file-list' },
              movie.files.map((file) => fileRow(file, ctx, several)),
            ),
          ),
        ),
      ),
    };
  },
};
