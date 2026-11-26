import { h } from '../dom.js';
import { toast } from './toast.js';

/** The small pill that shows a running scan. Returns the event handler. */
export function scanStatus() {
  const text = h('span');
  const el = h(
    'div',
    { class: 'scan-status', role: 'status', hidden: true },
    h('span', { class: 'spinner' }),
    text,
  );
  document.body.append(el);
  let timer;

  return ({ found, done, error }) => {
    clearTimeout(timer);
    if (error) {
      el.hidden = true;
      toast(error, { error: true });
      return;
    }
    const files = `${found} video${found === 1 ? '' : 's'}`;
    // A scan too quick to have reported progress only says so if it found
    // something.
    if (done && el.hidden && !found) return;
    el.hidden = false;
    el.classList.toggle('done', done);
    text.textContent = done ? `Scan complete · ${files}` : `Scanning… ${files} found`;
    if (done) {
      timer = setTimeout(() => {
        el.hidden = true;
      }, 2500);
    }
  };
}
