import { api, errorMessage } from '../api.js';
import { button, h } from '../dom.js';
import { state } from '../state.js';
import { attempt, toast } from '../components/toast.js';
import { keyField } from '../components/key-field.js';

const ATTRIBUTION = 'This product uses the TMDB API but is not endorsed or certified by TMDB.';

/**
 * TMDB LOGO SLOT.
 * The attribution requires the official TMDB logo. It is not shipped or drawn
 * here: download the approved asset from themoviedb.org (About > Logos and
 * Attribution) and save it as ui/img/tmdb-logo.svg. It stays hidden until that
 * file loads.
 */
function tmdbLogo() {
  const logo = h('img', { class: 'tmdb-logo', alt: 'TMDB logo', hidden: true });
  logo.addEventListener('load', () => {
    logo.hidden = false;
  });
  logo.addEventListener('error', () => logo.remove());
  logo.src = 'img/tmdb-logo.svg';
  return logo;
}

const section = (title, ...children) =>
  h('section', { class: 'settings-section' }, h('h2', { text: title }), children);

export default {
  load: () => api.getSettings(),
  build(settings, ctx) {
    const folder = h('p', { class: 'path' });
    const keyStatus = h('p', { class: 'muted' });

    const show = (next) => {
      state.settings = next;
      folder.textContent = next.media_folder ?? 'No folder chosen';
      keyStatus.textContent = next.has_tmdb_key
        ? 'A key is saved. Enter a new one to replace it.'
        : 'No key saved. Posters and details need a free TMDB key.';
      removeKey.hidden = !next.has_tmdb_key;
    };
    const reload = async () => show(await api.getSettings());

    const key = keyField({
      draft: ctx.ui,
      focusPrefix: 'settings',
      hasSavedKey: () => state.settings.has_tmdb_key,
    });
    const save = button('Save key', {
      variant: 'btn-primary',
      dataset: { focusKey: 'settings:save-key' },
      onClick: async () => {
        const value = key.value();
        if (!value) {
          key.say('Paste a key first.', true);
          return;
        }
        try {
          await api.setTmdbKey(value);
          key.clear();
          key.say('Key saved. Fetching anything that is missing.');
          await reload();
        } catch (error) {
          key.say(errorMessage(error), true);
        }
      },
    });
    const removeKey = button('Remove key', {
      dataset: { focusKey: 'settings:remove-key' },
      onClick: async () => {
        if (await attempt(() => api.setTmdbKey(''), 'TMDB key removed.')) {
          key.say('');
          await reload().catch(() => {});
        }
      },
    });
    key.actions.prepend(save);
    key.actions.append(removeKey);

    const action = (label, focus, run, success) => {
      const el = button(label, {
        dataset: { focusKey: `settings:${focus}` },
        onClick: async () => {
          el.disabled = true;
          await attempt(run, success);
          el.disabled = false;
        },
      });
      return el;
    };
    const link = (label, target) =>
      h('button', {
        type: 'button',
        class: 'link',
        text: label,
        dataset: { focusKey: `settings:link-${target}` },
        onClick: () => attempt(() => api.openExternal(target)),
      });

    const el = h(
      'div',
      { class: 'page settings' },
      h('h1', { text: 'Settings' }),
      section(
        'Media folder',
        folder,
        h(
          'div',
          { class: 'actions' },
          button('Change folder…', {
            dataset: { focusKey: 'settings:folder' },
            onClick: async () => {
              try {
                if (await api.pickMediaFolder()) {
                  await reload();
                  toast('Folder changed. Scanning…');
                }
              } catch (error) {
                toast(errorMessage(error), { error: true });
              }
            },
          }),
          action('Rescan now', 'rescan', () => api.rescan(), 'Rescan started.'),
        ),
      ),
      section(
        'TMDB API key',
        keyStatus,
        key.el,
        h('p', {}, link('How to get a free TMDB API key', 'tmdb_api')),
      ),
      section(
        'Metadata',
        h(
          'div',
          { class: 'actions' },
          action(
            'Refresh all metadata',
            'refresh-all',
            () => api.refreshAllMetadata(),
            'Refreshing metadata in the background.',
          ),
          action(
            'Clear image cache',
            'clear-cache',
            () => api.clearImageCache(),
            'Image cache cleared. Images will download again.',
          ),
        ),
      ),
      section(
        'About',
        h('p', { text: `Popcorn ${state.version}`.trim() }),
        h('p', {
          class: 'muted',
          text: 'Free software, licensed under the GNU General Public License v3.0 (GPL-3.0).',
        }),
        h('p', {}, link('Popcorn on GitHub', 'github')),
        h(
          'div',
          { class: 'attribution' },
          tmdbLogo(),
          h('p', { text: ATTRIBUTION }),
          link('themoviedb.org', 'tmdb'),
        ),
      ),
    );
    show(settings);

    // Updated in place so that a half-typed key survives library refreshes.
    return { el, update: show };
  },
};
