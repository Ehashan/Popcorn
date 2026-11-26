// First launch: choose the media folder, then (optionally) add a TMDB key.

import { api, errorMessage } from '../api.js';
import { button, h } from '../dom.js';
import { state } from '../state.js';
import { keyField } from '../components/key-field.js';
import { attempt, toastError } from '../components/toast.js';

export default {
  load: async () => null,
  build(_, ctx) {
    const path = h('p', { class: 'path' });
    const error = h('p', { class: 'field-status error', role: 'alert' });

    const choose = button('Choose your media folder', {
      variant: 'btn-primary',
      dataset: { focusKey: 'onboarding:folder', autofocus: '' },
      onClick: async () => {
        choose.disabled = true;
        error.textContent = '';
        try {
          const picked = await api.pickMediaFolder();
          if (picked) {
            state.settings.media_folder = picked;
            showFolder();
          }
        } catch (failure) {
          error.textContent = errorMessage(failure);
        }
        choose.disabled = false;
      },
    });

    const key = keyField({ draft: ctx.ui, focusPrefix: 'onboarding' });

    async function finish(withKey) {
      if (withKey && !key.value()) {
        key.say('Paste a key, or choose “Skip for now”.', true);
        return;
      }
      done.disabled = true;
      skip.disabled = true;
      try {
        if (withKey) await api.setTmdbKey(key.value());
        await api.finishOnboarding();
        state.settings = await api.getSettings();
        ctx.start();
      } catch (failure) {
        toastError(failure);
        done.disabled = false;
        skip.disabled = false;
      }
    }
    const done = button('Save and continue', {
      variant: 'btn-primary',
      dataset: { focusKey: 'onboarding:done' },
      onClick: () => finish(true),
    });
    const skip = button('Skip for now', {
      variant: 'btn-quiet',
      dataset: { focusKey: 'onboarding:skip' },
      onClick: () => finish(false),
    });

    const keyStep = h(
      'section',
      { class: 'step' },
      h('h2', { text: '2. Paste your TMDB API key' }),
      h('p', {
        class: 'muted',
        text: 'Popcorn uses it to fetch posters and details. It stays on this Mac.',
      }),
      key.el,
      h(
        'p',
        {},
        h('button', {
          type: 'button',
          class: 'link',
          text: 'How to get a free TMDB API key',
          dataset: { focusKey: 'onboarding:help' },
          onClick: () => attempt(() => api.openExternal('tmdb_api')),
        }),
      ),
      h('div', { class: 'actions' }, done, skip),
    );

    function showFolder() {
      const folder = state.settings.media_folder;
      path.textContent = folder ?? '';
      path.hidden = !folder;
      choose.lastChild.textContent = folder ? 'Change folder…' : 'Choose your media folder';
      choose.classList.toggle('btn-primary', !folder);
      keyStep.inert = !folder;
      keyStep.classList.toggle('disabled', !folder);
    }
    showFolder();

    return {
      el: h(
        'div',
        { class: 'onboarding' },
        h(
          'div',
          { class: 'onboarding-card' },
          h('h1', { text: 'Welcome to Popcorn' }),
          h('p', {
            class: 'muted',
            text: 'Your movies and series, in one place. Popcorn only reads your files.',
          }),
          h(
            'section',
            { class: 'step' },
            h('h2', { text: '1. Pick the folder with your videos' }),
            path,
            h('div', { class: 'actions' }, choose),
            error,
          ),
          keyStep,
        ),
      ),
      // Nothing here depends on the library.
      update: () => {},
    };
  },
};
