import { errorMessage } from '../api.js';
import { h } from '../dom.js';

const visible = new Map();

export function toast(message, { error = false } = {}) {
  let host = document.querySelector('.toasts');
  if (!host) {
    host = h('div', { class: 'toasts', role: 'status', 'aria-live': 'polite' });
    document.body.append(host);
  }
  // The same message twice only extends the first one.
  let entry = visible.get(message);
  if (entry) {
    clearTimeout(entry.timer);
  } else {
    entry = { el: h('div', { class: error ? 'toast toast-error' : 'toast', text: message }) };
    visible.set(message, entry);
    host.append(entry.el);
    entry.el.addEventListener('click', () => dismiss(message));
  }
  entry.timer = setTimeout(() => dismiss(message), error ? 7000 : 3500);
}

function dismiss(message) {
  const entry = visible.get(message);
  if (!entry) return;
  clearTimeout(entry.timer);
  entry.el.remove();
  visible.delete(message);
}

export const toastError = (error) => toast(errorMessage(error), { error: true });

/** Runs an action, showing a toast if it fails. Resolves to whether it worked. */
export async function attempt(action, success) {
  try {
    await action();
    if (success) toast(success);
    return true;
  } catch (error) {
    toastError(error);
    return false;
  }
}
