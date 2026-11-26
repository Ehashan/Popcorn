// Small DOM, image and formatting helpers.
// Library text (file names, TMDB text) is untrusted: it only ever reaches the
// page through textContent and element properties, never through markup.

import { fileUrl } from './api.js';

export function h(tag, props = {}, ...children) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value === null || value === undefined) continue;
    if (key === 'class') el.className = value;
    else if (key === 'text') el.textContent = value;
    else if (key === 'dataset') Object.assign(el.dataset, value);
    else if (key.startsWith('on')) el.addEventListener(key.slice(2).toLowerCase(), value);
    else if (key.includes('-') || key === 'role' || key === 'for') el.setAttribute(key, value);
    else el[key] = value;
  }
  append(el, ...children);
  return el;
}

export function append(el, ...children) {
  for (const child of children.flat(Infinity)) {
    if (child === null || child === undefined || child === false || child === '') continue;
    el.append(child);
  }
  return el;
}

const ICONS = {
  back: 'M15 5l-7 7 7 7',
  forward: 'M9 5l7 7-7 7',
  play: 'M8 5v14l11-7z',
  pause: 'M7 5h3.5v14H7zM13.5 5H17v14h-3.5z',
  next: 'M5 5v14l10-7zM16 5h3v14h-3z',
  check: 'M5 12.5l4.5 4.5L19 7.5',
  search: 'M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM15.5 15.5L20 20',
  settings: 'M4 7h10M18 7h2M4 17h2M10 17h10M16 5v4M8 15v4',
  volume: 'M4 9.5v5h4l5 4v-13l-5 4zM16.5 9a4 4 0 0 1 0 6M19 6.5a8 8 0 0 1 0 11',
  muted: 'M4 9.5v5h4l5 4v-13l-5 4zM16.500 9.500l5 5M21.500 9.500l-5 5',
  audio: 'M9 18V6l10-2v12M9 18a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0zM19 16a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0z',
  subtitles: 'M3.5 5.5h17v13h-17zM7 14.5h4M14 14.5h3M7 11h2M12 11h5',
  fullscreen: 'M9 4H4v5M15 4h5v5M20 15v5h-5M4 15v5h5',
  info: 'M12 4a8 8 0 1 0 0 16 8 8 0 0 0 0-16zM12 11v5M12 8v.01',
  close: 'M6 6l12 12M18 6L6 18',
};
const FILLED = new Set(['play', 'pause', 'next']);

export function icon(name) {
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('class', FILLED.has(name) ? 'icon icon-filled' : 'icon');
  svg.setAttribute('aria-hidden', 'true');
  const path = document.createElementNS(ns, 'path');
  path.setAttribute('d', ICONS[name]);
  svg.append(path);
  return svg;
}

export function button(label, { icon: iconName, variant = '', ...props } = {}) {
  return h(
    'button',
    { type: 'button', ...props, class: `btn ${variant} ${props.class ?? ''}`.trim() },
    iconName && icon(iconName),
    label && h('span', { text: label }),
  );
}

export function iconButton(iconName, label, props = {}) {
  return h(
    'button',
    {
      type: 'button',
      title: label,
      'aria-label': label,
      ...props,
      class: `icon-btn ${props.class ?? ''}`.trim(),
    },
    icon(iconName),
  );
}

export function hueOf(text) {
  let hash = 0;
  for (const char of String(text)) hash = (hash * 31 + char.codePointAt(0)) >>> 0;
  return hash % 360;
}

// Images that have loaded once are shown at once when a view is rebuilt, so a
// library refresh does not make the artwork fade in again.
const seenImages = new Set();

/**
 * A box showing a cached image over a generated placeholder. The placeholder
 * stays when there is no image or it fails to load.
 */
export function artwork(source, label, { className = '', caption = true, remote = false } = {}) {
  const box = h(
    'div',
    { class: `art ${className}`.trim() },
    caption && h('span', { class: 'art-label', text: label }),
  );
  box.style.setProperty('--hue', hueOf(label));
  const url = remote ? source : fileUrl(source);
  if (url) {
    const img = h('img', {
      alt: '',
      loading: 'lazy',
      decoding: 'async',
      draggable: false,
    });
    const loaded = () => {
      seenImages.add(url);
      box.classList.add('has-image');
    };
    img.addEventListener('load', loaded);
    img.addEventListener('error', () => {
      img.remove();
      box.classList.remove('has-image', 'instant');
    });
    if (seenImages.has(url)) box.classList.add('instant', 'has-image');
    img.src = url;
    box.append(img);
  }
  return box;
}

export function progressBar(ratio, className = '') {
  const fill = h('span', { class: 'progress-fill' });
  fill.style.transform = `scaleX(${Math.min(1, Math.max(0, ratio))})`;
  return h('span', { class: `progress ${className}`.trim() }, fill);
}

export function progressRatio(position, duration) {
  return duration > 0 ? position / duration : 0;
}

// Mirrors `progress::resume_position` in Rust.
export function resumeAt(position, duration) {
  return duration > 0 && position > 60 && position < duration * 0.9 ? position : null;
}

export function formatTime(seconds) {
  const total = Math.max(0, Math.floor(Number.isFinite(seconds) ? seconds : 0));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = String(total % 60).padStart(2, '0');
  return hours ? `${hours}:${String(minutes).padStart(2, '0')}:${secs}` : `${minutes}:${secs}`;
}

export function formatRuntime(minutes) {
  if (!minutes) return null;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (!hours) return `${rest} min`;
  return rest ? `${hours} h ${rest} min` : `${hours} h`;
}

export function formatSize(bytes) {
  if (!(bytes > 0)) return null;
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const exponent = Math.min(units.length - 1, Math.floor(Math.log10(bytes) / 3));
  const value = bytes / 1000 ** exponent;
  return `${value.toFixed(exponent >= 3 ? 1 : 0)} ${units[exponent]}`;
}

export function formatResolution(width, height) {
  return width && height ? `${width} × ${height}` : null;
}

export function episodeCode(season, episode) {
  return `S${season} E${episode}`;
}

export function formatRating(rating) {
  return rating > 0 ? `★ ${rating.toFixed(1)}` : null;
}
