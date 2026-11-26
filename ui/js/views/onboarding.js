// First launch: choose the media folder, then (optionally) add a TMDB key.

import { api, errorMessage } from '../api.js';
import { artwork, button, h, icon } from '../dom.js';
import { state } from '../state.js';
import { keyField } from '../components/key-field.js';
import { attempt, toastError } from '../components/toast.js';

const WALL_TILES = 40;

/** A tilted wall of generated posters behind the card. */
const wall = () =>
  h(
    'div',
    { class: 'wall', 'aria-hidden': 'true' },
    Array.from({ length: WALL_TILES }, (_, i) =>
      artwork(null, '', { caption: false, seed: `wall ${i * 7}` }),
    ),
  );

function step(number, title, ...body) {
  const badge = h('span', { class: 'number', text: String(number) });
  const el = h('li', { class: 'ob-step' }, badge, h('div', {}, h('b', { text: title }), body));
  const setDone = (done) => {
    el.classList.toggle('done', done);
    badge.replaceChildren(done ? icon('check') : String(number));
  };
  return { el, setDone };
}

export default {
  load: async () => null,
  build(_, ctx) {
    const path = h('span', { class: 'path' });
    const hint = h('span', { text: 'Movies and episodes in any subfolder are found automatically.' });
    const error = h('p', { class: 'field-status error', role: 'alert' });
    const choose = button('Choose folder', {
      icon: 'folder',
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

    const key = keyField({
      draft: ctx.ui,
      focusPrefix: 'onboarding',
      hasSavedKey: () => state.settings.has_tmdb_key,
    });

    async function finish(withKey) {
      done.disabled = true;
      skip.disabled = true;
      try {
        if (withKey && key.value()) await api.setTmdbKey(key.value());
        await api.finishOnboarding();
        state.settings = await api.getSettings();
        ctx.start();
      } catch (failure) {
        toastError(failure);
        showFolder();
      }
    }
    const done = button('Open my library', {
      variant: 'btn-play',
      dataset: { focusKey: 'onboarding:done' },
      onClick: () => finish(true),
    });
    const skip = button('Skip the key for now', {
      variant: 'btn-quiet',
      dataset: { focusKey: 'onboarding:skip' },
      onClick: () => finish(false),
    });

    const folderStep = step(
      1,
      'Choose your media folder',
      h('p', {}, hint, path),
      choose,
      error,
    );
    const keyStep = step(
      2,
      'Add your TMDB key',
      h(
        'p',
        {},
        'For posters and details. It stays on this Mac. ',
        h('button', {
          type: 'button',
          class: 'link link-accent',
          text: 'Get a free key',
          dataset: { focusKey: 'onboarding:help' },
          onClick: () => attempt(() => api.openExternal('tmdb_api')),
        }),
      ),
      key.el,
    );

    function showFolder() {
      const folder = state.settings.media_folder;
      path.textContent = folder ?? '';
      path.hidden = !folder;
      hint.hidden = Boolean(folder);
      choose.lastChild.textContent = folder ? 'Choose another folder' : 'Choose folder';
      choose.className = `btn btn-sm ${folder ? 'btn-quiet' : 'btn-play'}`;
      folderStep.setDone(Boolean(folder));
      // Neither way forward makes sense without a folder.
      done.disabled = !folder;
      skip.disabled = !folder;
    }
    showFolder();

    const brand = document.querySelector('#topbar .logo').cloneNode(true);
    return {
      el: h(
        'div',
        { class: 'onboard' },
        wall(),
        h(
          'div',
          { class: 'ob-card' },
          h('div', { class: 'brand' }, brand, 'Popcorn'),
          h('h1', { text: 'Your movies, on your Mac' }),
          h('p', {
            class: 'ob-intro',
            text: 'Point Popcorn at the folder where you keep your movies and shows. Nothing is moved or uploaded.',
          }),
          h('ol', { class: 'ob-steps' }, folderStep.el, keyStep.el),
          h('div', { class: 'actions' }, done, skip),
        ),
      ),
      // Nothing here depends on the library.
      update: () => {},
    };
  },
};
