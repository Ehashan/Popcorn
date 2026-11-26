import { state } from '../state.js';
import { toast } from './toast.js';

/** The "Scanning library" note in the header. Returns the event handler. */
export function scanStatus() {
  const el = document.getElementById('scan');
  return ({ done, error }) => {
    el.hidden = done;
    if (error) toast(error, { error: true });
    else if (done) state.lastScan = new Date();
  };
}
