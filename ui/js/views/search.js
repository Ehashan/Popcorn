// Instant local search across titles, cast and episode names.

import { api } from '../api.js';
import { h, icon } from '../dom.js';
import { posterGrid } from '../components/poster-grid.js';
import { toastError } from '../components/toast.js';

export default {
  load: () => api.listTitles(),
  build(data, ctx) {
    let byId = new Map(data.map((t) => [t.id, t]));
    let latest = 0;
    const grid = posterGrid(ctx, 'search');
    const message = h('p', { class: 'empty', 'aria-live': 'polite' });
    const input = h('input', {
      type: 'text',
      class: 'search-input',
      placeholder: 'Titles, cast or episode names',
      'aria-label': 'Search the library',
      value: ctx.ui.query ?? '',
      spellcheck: false,
      autocomplete: 'off',
      autocapitalize: 'off',
      dataset: { focusKey: 'search-input', autofocus: '' },
      onInput: () => {
        ctx.ui.query = input.value;
        run(true);
      },
      onKeydown: (event) => {
        if (event.key === 'Escape' && input.value) {
          event.preventDefault();
          event.stopPropagation();
          input.value = '';
          ctx.ui.query = '';
          run(true);
        } else if (event.key === 'ArrowDown' || event.key === 'Enter') {
          const first = grid.el.querySelector('[data-nav-item]');
          if (first) {
            event.preventDefault();
            first.focus();
          }
        }
      },
    });

    async function run(reset) {
      const query = input.value.trim();
      const request = ++latest;
      let ids = [];
      if (query) {
        try {
          ids = await api.searchLibrary(query);
        } catch (error) {
          toastError(error);
        }
        // A newer keystroke has its own request on the way.
        if (request !== latest) return;
      }
      const results = ids.map((id) => byId.get(id)).filter(Boolean);
      if (reset) grid.reset(results);
      else grid.set(results);
      message.hidden = results.length > 0;
      message.textContent = query ? `No results for “${query}”.` : 'Start typing to search.';
    }

    const el = h(
      'div',
      { class: 'page' },
      h('div', { class: 'search-box' }, icon('search'), input),
      message,
      grid.el,
    );

    return {
      el,
      ready: () => run(false),
      update(next) {
        byId = new Map(next.map((t) => [t.id, t]));
        return run(false);
      },
      destroy: () => {
        latest += 1;
        grid.destroy();
      },
    };
  },
};
