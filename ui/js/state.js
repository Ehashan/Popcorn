// The one shared store.

export const state = {
  /** `get_settings` result. */
  settings: { media_folder: null, has_tmdb_key: false, onboarded: false },
  version: '',
  /** Navigation history; the last entry is the current screen. */
  history: [],
  /** Sort and filters per grid, kept while the app runs. */
  gridPrefs: {
    movie: { sort: 'title', genre: '', watched: 'all' },
    show: { sort: 'title', genre: '', watched: 'all' },
  },
  /** True while the player covers the app. */
  playing: false,
};
