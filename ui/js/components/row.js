import { h, iconButton } from '../dom.js';

/** A titled, horizontally scrolling strip. `key` identifies it across rebuilds. */
export function row({ title, key, items, onSeeAll }) {
  const scroller = h(
    'div',
    { class: 'row-scroller', dataset: { scrollKey: key, navGroup: 'row' } },
    items,
  );
  const page = (direction) =>
    scroller.scrollBy({ left: direction * scroller.clientWidth * 0.85, behavior: 'smooth' });
  return h(
    'section',
    { class: 'row' },
    h(
      'header',
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
    h(
      'div',
      { class: 'row-body' },
      scroller,
      iconButton('back', `Scroll ${title} left`, {
        class: 'row-arrow row-arrow-left',
        tabIndex: -1,
        onClick: () => page(-1),
      }),
      iconButton('forward', `Scroll ${title} right`, {
        class: 'row-arrow row-arrow-right',
        tabIndex: -1,
        onClick: () => page(1),
      }),
    ),
  );
}
