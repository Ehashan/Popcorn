// Search TMDB and pick the right title by hand.

import { api, errorMessage } from '../api.js';
import { artwork, button, h } from '../dom.js';
import { toast } from './toast.js';

export function openFixMatch(title, onDone) {
  const opener = document.activeElement?.dataset?.focusKey;
  let request = 0;
  let saving = false;
  let debounce;

  const input = h('input', {
    class: 'field',
    type: 'text',
    value: title.title,
    'aria-label': 'Search TMDB',
    spellcheck: false,
    autocomplete: 'off',
    onInput: () => {
      clearTimeout(debounce);
      debounce = setTimeout(search, 300);
    },
    onKeydown: (event) => {
      if (event.key !== 'Enter') return;
      event.preventDefault();
      clearTimeout(debounce);
      search();
    },
  });
  const status = h('p', { class: 'field-status', role: 'status' });
  const results = h('ul', { class: 'matches' });

  const say = (text, error = false) => {
    status.textContent = text;
    status.classList.toggle('error', error);
  };

  const what = title.kind === 'show' ? 'series' : 'movie';
  const dialog = h(
    'dialog',
    { class: 'dialog', 'aria-label': 'Fix match' },
    h('h2', { text: 'Fix match' }),
    h('p', {
      class: 'dialog-intro',
      text: `Wrong poster or details? Search TMDB and pick the right ${what}.`,
    }),
    input,
    status,
    results,
    h(
      'div',
      { class: 'dialog-foot' },
      button('Cancel', { variant: 'btn-quiet btn-sm', onClick: () => dialog.close() }),
    ),
  );

  async function search() {
    const query = input.value.trim();
    if (!query) return;
    const current = ++request;
    results.replaceChildren();
    say('Searching…');
    try {
      const matches = await api.tmdbSearch(title.kind, query);
      if (current !== request) return;
      say(matches.length ? '' : 'No results. Try a different title.');
      results.replaceChildren(...matches.map(matchRow));
    } catch (error) {
      if (current === request) say(errorMessage(error), true);
    }
  }

  function matchRow(match) {
    const kind = title.kind === 'show' ? 'TV series' : 'Movie';
    return h(
      'li',
      {},
      h(
        'button',
        { type: 'button', class: 'match', onClick: () => choose(match) },
        artwork(match.poster_url, match.title, { caption: false, remote: true }),
        h(
          'span',
          {},
          h('b', { text: match.title }),
          h('span', { class: 'match-sub', text: `${match.year ?? 'Year unknown'}, ${kind}` }),
        ),
        match.tmdb_id === title.tmdb_id &&
          h('span', { class: 'match-current', text: 'Current match' }),
      ),
    );
  }

  async function choose(match) {
    if (saving) return;
    saving = true;
    dialog.classList.add('busy');
    say('Fetching details…');
    try {
      await api.fixMatch(title.id, match.tmdb_id);
      toast('Match updated.');
      dialog.close();
      onDone();
    } catch (error) {
      say(errorMessage(error), true);
    } finally {
      saving = false;
      dialog.classList.remove('busy');
    }
  }

  dialog.addEventListener('close', () => {
    clearTimeout(debounce);
    request += 1;
    dialog.remove();
    // The button that opened the dialog may have been rebuilt meanwhile.
    if (opener) {
      document.querySelector(`[data-focus-key="${opener}"]`)?.focus({ preventScroll: true });
    }
  });
  document.body.append(dialog);
  dialog.showModal();
  input.select();
  search();
}
