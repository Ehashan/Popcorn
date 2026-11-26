import { api } from '../api.js';
import { button, formatResolution, formatRuntime, formatSize, formatTime, h, resumeAt } from '../dom.js';
import {
  castSection,
  detailsPage,
  fact,
  revealButton,
  titleActions,
} from '../components/details-parts.js';
import { attempt } from '../components/toast.js';

/** "4K" or "1080p", from the video size if played, else from the file name. */
function quality({ width, height, filename }) {
  if (width && height) {
    if (width >= 3800) return '4K';
    if (width >= 1900) return '1080p';
    if (width >= 1260) return '720p';
    return `${height}p`;
  }
  const tag = /\b(2160p|4k|1080p|720p)\b/i.exec(filename.replace(/[._]/g, ' '))?.[1].toLowerCase();
  if (!tag) return null;
  return tag === '2160p' || tag === '4k' ? '4K' : tag;
}

function playActions(movie, ctx) {
  const { play } = movie;
  if (!play) return [];
  const resume = play.resume_position;
  return [
    button(resume ? `Resume from ${formatTime(resume)}` : 'Play', {
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

function fileInfo(file, ctx, several) {
  return h(
    'li',
    {},
    h(
      'dl',
      { class: 'file-info' },
      h('div', { class: 'file-name' }, h('dt', { text: 'Filename' }), h('dd', { text: file.filename })),
      fact('Size', formatSize(file.size)),
      fact('Video', formatResolution(file.width, file.height)),
      fact('Length', file.duration && formatRuntime(Math.round(file.duration / 60))),
      several &&
        h(
          'div',
          { class: 'file-actions' },
          button('Play', {
            icon: 'play',
            variant: 'btn-sm',
            dataset: { focusKey: `file-play:${file.file_id}` },
            onClick: () => ctx.play(file.file_id, resumeAt(file.position, file.duration)),
          }),
          button('Show in Finder', {
            variant: 'btn-quiet btn-sm',
            dataset: { focusKey: `file-reveal:${file.file_id}` },
            onClick: () => attempt(() => api.revealFile(file.file_id)),
          }),
        ),
    ),
  );
}

export default {
  load: (params) => api.getTitle(params.id),
  build(movie, ctx) {
    const several = movie.files.length > 1;
    const main = movie.files.find((f) => f.file_id === movie.play?.file_id) ?? movie.files[0];
    const label = main && quality(main);
    return {
      el: detailsPage(movie, ctx, {
        facts: [formatRuntime(movie.runtime)],
        actions: [
          playActions(movie, ctx),
          titleActions(movie, ctx),
          main && revealButton(main.file_id, ctx),
        ],
        extraCredit: label && ['Quality', label],
        sections: [
          castSection(movie.cast),
          h('h2', { class: 'section-title', text: several ? 'Files' : 'File' }),
          h(
            'ul',
            { class: 'files' },
            movie.files.map((file) => fileInfo(file, ctx, several)),
          ),
        ],
      }),
    };
  },
};
