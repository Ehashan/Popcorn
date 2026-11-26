// The one shared store.

export const state = {
  /** `get_settings` result. */
  settings: { media_folder: null, has_tmdb_key: false, onboarded: false },
  version: '',
  /** Navigation history; the last entry is the current screen. */
  history: [],
  /** Sort and filters per grid, kept while the app runs. */
  gridPrefs: {
    movie: { sort: 'added', genre: '', watched: 'all' },
    show: { sort: 'added', genre: '', watched: 'all' },
  },
  /** True while the player covers the app. */
  playing: false,
  /** When the first files were added, in Unix seconds. See `isNew`. */
  libraryStart: 0,
  /** When the last scan this session finished. */
  lastScan: null,
};
