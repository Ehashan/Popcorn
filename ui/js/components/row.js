import { h, icon } from '../dom.js';

/** A titled, horizontally scrolling strip. `key` identifies it across rebuilds. */
export function row({ title, key, items, onSeeAll }) {
  const scroller = h(
    'div',
    { class: 'scroller', dataset: { scrollKey: key, navGroup: 'row' } },
    items,
  );
  const arrow = (direction, name, side, label) =>
    h(
      'button',
      {
        type: 'button',
        class: `row-arrow ${side}`,
        tabIndex: -1,
        'aria-label': `Scroll ${title} ${label}`,
        onClick: () =>
          scroller.scrollBy({ left: direction * scroller.clientWidth * 0.85, behavior: 'smooth' }),
      },
      icon(name),
    );
  const previous = arrow(-1, 'back', 'prev', 'left');
  const next = arrow(1, 'forward', 'next', 'right');
  // An arrow is only offered when there is somewhere to go.
  const update = () => {
    previous.disabled = scroller.scrollLeft < 8;
    next.disabled = scroller.scrollLeft + scroller.clientWidth >= scroller.scrollWidth - 8;
  };
  scroller.addEventListener('scroll', update, { passive: true });
  new ResizeObserver(update).observe(scroller);

  const wrap = h('div', { class: 'row-wrap' }, previous, scroller, next);
  return h(
    'section',
    { class: 'row', 'aria-label': title },
    h(
      'div',
      { class: 'row-head' },
      h('h2', { text: title }),
      onSeeAll &&
        h('button', {
          type: 'button',
          class: 'link',
          text: 'See all',
          dataset: { focusKey: `see-all:${key}` },
          onClick: onSeeAll,
        }),
    ),
    wrap,
  );
}
