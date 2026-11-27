export function symbolBox(text) {
  const box = text.trim().split(/[\s,]+/).map(Number);
  return box.length === 4 && box.every(Number.isFinite) && box[2] > 0 && box[3] > 0 ? box : [0, 0, 24, 24];
}

// A path-only template uses one em along the longer side of its viewBox.
export function symbolSize(text, point, source) {
  if (!source?.startsWith('symbol:icon||')) return [point, point];
  const [, , w, h] = symbolBox(text);
  return [point * (w / Math.max(w, h)), point * (h / Math.max(w, h))];
}

export function symbolPlaceholder(width, height) {
  const ratio = width === height ? '' : ` viewBox="0 0 ${width} ${height}"`;
  return `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"${ratio}/>`)}`;
}

export function symbolMask(path, filled, nonzero, box, width, height, weight, mirror) {
  const [minX, minY, w, h] = symbolBox(box);
  const stroke = 1.1 + (Math.max(100, Math.min(900, weight)) - 100) / 400;
  const paint = filled
    ? `fill="black" fill-rule="${nonzero ? 'nonzero' : 'evenodd'}"`
    : `fill="none" stroke="black" stroke-width="${stroke}" stroke-linecap="round" stroke-linejoin="round"`;
  const shapes = path.split(';').map((d) => `<path d="${d}" ${paint}/>`).join('');
  const body = mirror ? `<g transform="translate(${2 * minX + w} 0) scale(-1 1)">${shapes}</g>` : shapes;
  return `url("data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" ${width === height ? `width="${width}" height="${height}" ` : ''}viewBox="${minX} ${minY} ${w} ${h}">${body}</svg>`)}")`;
}

const pendingRefits = new Set();
function refitAfterPlaceholder({ currentTarget: el }) {
  const refit = el.symbolRefit;
  el.symbolRefit = null;
  if (!el.isConnected || el.getAttribute('src') !== el.symbolPlaceholder || pendingRefits.has(refit)) return;
  pendingRefits.add(refit);
  requestAnimationFrame(() => { pendingRefits.delete(refit); refit(); });
}

export function syncSymbolPlaceholder(el, refit) {
  const src = el.symbolPlaceholder;
  if (el.getAttribute('src') === src) return;
  el.src = src;
  if (el.complete && el.currentSrc === el.src) return;
  // An uncached placeholder can change the layout after the commit's fit pass.
  el.symbolRefit = refit;
  el.addEventListener('load', refitAfterPlaceholder, { once: true });
}

function fitsContentBox(cs, width, height) {
  let w = parseFloat(cs.width), h = parseFloat(cs.height);
  if (cs.boxSizing === 'border-box') {
    w -= parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight) + parseFloat(cs.borderLeftWidth) + parseFloat(cs.borderRightWidth);
    h -= parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom) + parseFloat(cs.borderTopWidth) + parseFloat(cs.borderBottomWidth);
  }
  return width <= w && height <= h;
}

// SVG's default preserveAspectRatio meets the viewBox inside a filled viewport.
export function symbolFit(el, width, height, fit) {
  const cs = getComputedStyle(el);
  const fits = fit === 'scale-down' && fitsContentBox(cs, width, height);
  return [fit === 'none' || fit === 'scale-down' && fits ? `${width}px ${height}px`
    : fit === 'fill' ? '100% 100%' : fit === 'scale-down' ? 'contain' : fit,
    cs.objectPosition || '50% 50%'];
}

const rasterMaskSize = {
  fill: '100% 100%',
  contain: 'contain',
  cover: 'cover',
  none: 'auto',
  'scale-down': 'var(--exact-tint-fit,contain)',
};

// A raster mask follows the computed fit and position, using the decoded size for scale-down.
export function tintFit(el) {
  if (!globalThis.CSS?.supports || !el.style.getPropertyValue('mask-size')) return;
  const cs = getComputedStyle(el);
  const fit = cs.objectFit;
  const fits = fit === 'scale-down' && el.complete && fitsContentBox(cs, el.naturalWidth, el.naturalHeight);
  // Validate after var() substitution: an invalid mask-position would reset to top left.
  const position = cs.getPropertyValue('--exact-object-position').trim();
  el.style.setProperty('mask-size', rasterMaskSize[fit]);
  el.style.setProperty('mask-position', CSS.supports('object-position', position) ? position : '50% 50%');
  if (fit !== 'scale-down') return;
  if (!el.complete) { el.addEventListener('load', () => tintFit(el), { once: true }); return; }
  el.style.setProperty('--exact-tint-fit', fits ? 'auto' : 'contain');
}
