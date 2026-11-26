// Arrow keys move between cards in rows, grids and lists.

const ITEM = '[data-nav-item]';

const centerX = (el) => {
  const box = el.getBoundingClientRect();
  return box.left + box.width / 2;
};

function neighbour(item, group, key) {
  const items = [...group.querySelectorAll(ITEM)];
  const index = items.indexOf(item);
  const type = group.dataset.navGroup;
  const horizontal = key === 'ArrowLeft' || key === 'ArrowRight';
  const step = key === 'ArrowLeft' || key === 'ArrowUp' ? -1 : 1;

  if (type === 'list') return horizontal ? null : items[index + step];
  if (horizontal) return items[index + step];
  if (type === 'grid') {
    const columns = items.filter((el) => el.offsetTop === items[0].offsetTop).length;
    const target = index + step * columns;
    // The last grid line may be short.
    if (step > 0 && target >= items.length) {
      return items.at(-1).offsetTop > item.offsetTop ? items.at(-1) : null;
    }
    return items[target];
  }
  // Rows: the card closest above or below in the neighbouring row.
  const groups = [...document.querySelectorAll('#app [data-nav-group="row"]')].filter((el) =>
    el.querySelector(ITEM),
  );
  const next = groups[groups.indexOf(group) + step];
  if (!next) return null;
  const x = centerX(item);
  return [...next.querySelectorAll(ITEM)].reduce((best, el) =>
    Math.abs(centerX(el) - x) < Math.abs(centerX(best) - x) ? el : best,
  );
}

export function moveFocus(event) {
  if (!event.key.startsWith('Arrow') || event.shiftKey) return;
  const item = event.target.closest?.(ITEM);
  const group = item?.closest('[data-nav-group]');
  if (!group) return;
  const next = neighbour(item, group, event.key);
  if (!next) return;
  event.preventDefault();
  next.focus({ preventScroll: true });
  next.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}
