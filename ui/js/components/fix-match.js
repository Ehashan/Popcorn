// Search TMDB and pick the right title by hand.

import { api, errorMessage } from '../api.js';
import { artwork, button, h, iconButton } from '../dom.js';
import { toast } from './toast.js';

export function openFixMatch(title, onDone) {
  const opener = document.activeElement?.dataset?.focusKey;
  let request = 0;
  let saving = false;

  const input = h('input', {
    type: 'text',
    value: title.title,
    'aria-label': 'Search TMDB',
    spellcheck: false,
    autocomplete: 'off',
  });
  const status = h('p', { class: 'dialog-status', role: 'status' });
  const results = h('ul', { class: 'match-list' });

  const say = (text, error = false) => {
    status.textContent = text;
    status.classList.toggle('error', error);
  };

  const dialog = h(
    'dialog',
    { class: 'dialog', 'aria-label': 'Fix match' },
    h(
      'header',
      { class: 'dialog-head' },
      h('h2', { text: 'Fix match' }),
      iconButton('close', 'Close', { onClick: () => dialog.close() }),
    ),
    h(
      'form',
      {
        class: 'input-row',
        onSubmit: (event) => {
          event.preventDefault();
          search();
        },
      },
      input,
      button('Search', { type: 'submit', variant: 'btn-primary' }),
    ),
    status,
    results,
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
    return h(
      'li',
      {},
      h(
        'button',
        { type: 'button', class: 'match', onClick: () => choose(match) },
        artwork(match.poster_url, match.title, {
          className: 'poster match-poster',
          caption: false,
          remote: true,
        }),
        h(
          'span',
          { class: 'match-text' },
          h('span', { class: 'card-title', text: match.title }),
          h('span', { class: 'card-sub', text: match.year ? String(match.year) : 'Year unknown' }),
        ),
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
