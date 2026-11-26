// The TMDB key input with its "Test key" button, used by onboarding and
// settings.

import { api, errorMessage } from '../api.js';
import { button, h } from '../dom.js';

/** `draft.key` keeps the typed text when the screen is rebuilt. */
export function keyField({ draft, focusPrefix, hasSavedKey = () => false }) {
  const input = h('input', {
    class: 'field',
    type: 'password',
    placeholder: 'API key or read access token',
    'aria-label': 'TMDB API key',
    value: draft.key ?? '',
    spellcheck: false,
    autocomplete: 'off',
    autocapitalize: 'off',
    dataset: { focusKey: `${focusPrefix}:key` },
    onInput: () => {
      draft.key = input.value;
      say('');
    },
  });
  const status = h('p', { class: 'field-status', role: 'status' });
  function say(text, error = false) {
    status.textContent = text;
    status.classList.toggle('error', error);
  }
  const test = button('Test key', {
    variant: 'btn-sm',
    dataset: { focusKey: `${focusPrefix}:test` },
    onClick: async () => {
      const typed = input.value.trim();
      if (!typed && !hasSavedKey()) {
        say('Paste a key first.', true);
        return;
      }
      test.disabled = true;
      say('Testing…');
      try {
        await api.testTmdbKey(typed || null);
        say(typed ? 'This key works.' : 'The saved key works.');
      } catch (error) {
        say(errorMessage(error), true);
      }
      test.disabled = false;
    },
  });
  const actions = h('div', { class: 'set-controls' }, test);
  return {
    el: h('div', { class: 'key-field' }, input, actions, status),
    actions,
    say,
    value: () => input.value.trim(),
    clear: () => {
      input.value = '';
      draft.key = '';
    },
  };
}
