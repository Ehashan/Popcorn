// The simple shapes drawn on generated artwork. Colors come from CSS classes.

import { svg } from '../dom.js';

const circle = (cx, cy, r, className) => svg('circle', { cx, cy, r, class: className });
const path = (d, className) => svg('path', { d, class: className });
const range = (length) => Array.from({ length }, (_, i) => i);

const MOTIFS = [
  // A setting sun, sliced by the horizon.
  (w, h) => {
    const [cx, cy, r] = [w * 0.64, h * 0.4, Math.min(w, h) * 0.26];
    const slices = range(6).map((i) => {
      const y = cy + r * 0.15 + i * r * 0.16;
      const height = 1.2 + i * 0.9;
      const reach = Math.max(Math.abs(y - cy), Math.abs(y + height - cy));
      const half = Math.sqrt(Math.max(0, r * r - reach * reach));
      return svg('rect', { x: cx - half, y, width: half * 2, height, class: 'shade' });
    });
    return [circle(cx, cy, r, 'glow'), ...slices];
  },
  // Rings.
  (w, h) => range(7).map((i) => circle(w * 0.7, h * 0.32, (i + 1) * Math.min(w, h) * 0.085, 'stroke')),
  // A perspective grid.
  (w, h) => {
    const [x, y] = [w * 0.5, h * 0.52];
    return [
      ...range(13).map((i) => path(`M${x} ${y}L${x + (i - 6) * w * 0.2} ${h}`, 'stroke')),
      ...range(6).map((i) => path(`M0 ${y + ((h - y) * (i + 1) ** 2) / 36}H${w}`, 'stroke')),
      circle(x, y - h * 0.12, Math.min(w, h) * 0.13, 'glow'),
    ];
  },
  // Waves.
  (w, h) =>
    range(9).map((i) => {
      const y = h * 0.35 + i * h * 0.06;
      return path(`M0 ${y}Q${w * 0.25} ${y - h * 0.035} ${w * 0.5} ${y}T${w} ${y}`, 'stroke');
    }),
  // A skyline.
  (w, h, random) => {
    const towers = [];
    for (let x = 0; x < w; ) {
      const width = w * (0.05 + random() * 0.08);
      const height = h * (0.12 + random() * 0.3);
      towers.push(svg('rect', { x, y: h - height, width, height, class: 'shade' }));
      x += width + w * 0.01;
    }
    return [...towers, circle(w * 0.3, h * 0.3, Math.min(w, h) * 0.07, 'glow')];
  },
  // A crescent moon over hills.
  (w, h) => {
    const [cx, cy, r] = [w * 0.68, h * 0.3, Math.min(w, h) * 0.18];
    return [
      circle(cx, cy, r, 'glow'),
      circle(cx + r * 0.4, cy - r * 0.15, r * 0.9, 'shade'),
      path(
        `M0 ${h * 0.82}Q${w * 0.3} ${h * 0.7} ${w * 0.55} ${h * 0.8}T${w} ${h * 0.74}V${h}H0z`,
        'shade',
      ),
    ];
  },
];

/** A small seeded generator, so a title's artwork never changes. */
function seeded(seed) {
  let value = seed || 1;
  return () => {
    value = (Math.imul(value, 1664525) + 1013904223) >>> 0;
    return value / 4294967296;
  };
}

export function motif(hash, wide) {
  const [w, h] = wide ? [160, 90] : [100, 150];
  const draw = MOTIFS[(hash >>> 4) % MOTIFS.length];
  return svg(
    'svg',
    {
      class: 'motif',
      viewBox: `0 0 ${w} ${h}`,
      preserveAspectRatio: 'xMidYMid slice',
      'aria-hidden': 'true',
    },
    ...draw(w, h, seeded(hash)),
  );
}
