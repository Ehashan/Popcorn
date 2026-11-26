// The dark and light themes. `theme-boot.js` applies the saved one at startup;
// the colors themselves are in css/tokens.css.

const STORAGE_KEY = 'popcorn.theme';
export const THEME_CHANGED = 'themechange';

export const currentTheme = () => document.documentElement.dataset.theme;

/** Makes the native parts (traffic lights, menus, dialogs) match the page. */
export function syncWindowTheme() {
  window.__TAURI__?.window
    ?.getCurrentWindow()
    .setTheme(currentTheme())
    .catch((error) => console.warn('could not set the window theme', error));
}

export function setTheme(theme) {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem(STORAGE_KEY, theme);
  syncWindowTheme();
  window.dispatchEvent(new CustomEvent(THEME_CHANGED));
}

export const toggleTheme = () => setTheme(currentTheme() === 'dark' ? 'light' : 'dark');
