// The 16:9 card for continue watching and episode search results.

import {
  artwork,
  episodeCode,
  formatLeft,
  h,
  icon,
  iconButton,
  progressBar,
  progressRatio,
} from '../dom.js';

/** `item` is a `ContinueItem`. Clicking plays it. */
export function wideCard(item, ctx, keyPrefix) {
  const code = item.season == null ? null : episodeCode(item.season, item.episode);
  const sub = code ? [code, item.episode_name].filter(Boolean).join('  ') : 'Movie';
  const start = item.position > 0 ? item.position : null;
  const note = start && item.duration ? formatLeft(start, item.duration) : code && 'Up next';
  return h(
    'div',
    { class: 'wide-card' },
    h(
      'button',
      {
        type: 'button',
        class: 'wide',
        'aria-label': `${start ? 'Resume' : 'Play'} ${item.title} ${sub}`,
        dataset: { focusKey: `${keyPrefix}:${item.file_id}`, navItem: '' },
        onClick: () => ctx.play(item.file_id, start),
      },
      h(
        'span',
        { class: 'wide-art' },
        artwork(item.still_path ?? item.backdrop_path, item.title, { caption: false, wide: true }),
        h('span', { class: 'wide-play' }, icon('play')),
        note && h('span', { class: 'wide-left', text: note }),
        start && progressBar(progressRatio(start, item.duration)),
      ),
      h('span', { class: 'wide-title', text: item.title }),
      h('span', { class: 'wide-sub', text: sub }),
    ),
    iconButton('info', `Details for ${item.title}`, {
      class: 'wide-info',
      dataset: { focusKey: `${keyPrefix}-info:${item.file_id}` },
      onClick: () => ctx.openTitle({ id: item.title_id, kind: item.kind }),
    }),
  );
}
