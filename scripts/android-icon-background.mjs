// Android's adaptive icon background (LLP 1163 D6). At a 48 px draw the artwork spans
// 36⅔ px. Sixteen intervals put the samples 2.29 launcher pixels apart.
import { encodePng } from './png.mjs';

// The adaptive icon's layer, 108 dp, and the maskable image on it, 82.5 dp, in pixels at xxxhdpi (4 px a dp). At
// 82.5 dp the image's W3C safe zone (the centred circle of 80% diameter) is the layer's 66 dp one (developer.android.com,
// "Adaptive icons").
export const LAYER = 432, MASKABLE = 330;

const CELLS = 16;
const sizes = { ldpi: 81, mdpi: 108, tvdpi: 144, hdpi: 162, xhdpi: 216, xxhdpi: 324, xxxhdpi: 432 };
const rgb = data => '#FF' + Buffer.from(data.subarray(0, 3)).toString('hex').toUpperCase();

// The convolution of two box filters (2/3 and 1/6 of a cell) is a trapezoid.
// Integrating it over each source pixel includes fractional pixels. Its tapered
// ends give a narrow rim proportionally little weight as it enters the window.
// The integrated rising ramp starts at 1/12, with coefficient 1 / (2 × 2/3 × 1/6).
function integral(x) {
  if (x > 0.5) return 1 - integral(1 - x);
  const t = Math.max(0, x - 1 / 12);
  return 4.5 * (t * t - Math.max(0, t - 1 / 6) ** 2);
}

function colourGrid(image) {
  const side = CELLS + 1, cell = image.width / CELLS;
  const windows = Array.from({ length: side }, (_, i) => {
    const centre = Math.max(0.5, Math.min(CELLS - 0.5, i));
    const left = (centre - 0.5) * cell, taps = [];
    for (let p = Math.floor(left); p < Math.min(image.width, Math.ceil(left + cell)); p++) {
      const u = (p - left) / cell;
      const weight = integral(u + 1 / cell) - integral(u);
      taps.push({ p, weight, offset: u + 0.5 / cell - 0.5 });
    }
    return taps;
  });
  function line(read) {
    const means = windows.map(taps => taps.reduce((sum, { p, weight }) => sum + weight * read(p), 0));
    for (const edge of [0, CELLS]) {
      const inward = edge === 0 ? 1 : -1, near = means[edge + inward], next = means[edge + 2 * inward];
      // The end window is half a cell inside the artwork. Project its mean to
      // the edge only as far as its own slope and both adjoining intervals agree.
      let covariance = 0, variance = 0;
      for (const { p, weight, offset } of windows[edge]) {
        covariance += weight * offset * (read(p) - means[edge]);
        variance += weight * offset * offset;
      }
      const local = -0.5 * inward * covariance / variance;
      const first = means[edge] - near, second = (near - next) / 2;
      means[edge] += Math.max(0, Math.min(local, first, second)) + Math.min(0, Math.max(local, first, second));
    }
    return means;
  }
  const stride = 3 * side, rows = new Float64Array(image.height * stride);
  for (let y = 0; y < image.height; y++) for (let c = 0; c < 3; c++) {
    const samples = line(x => image.data[4 * (y * image.width + x) + c]);
    for (let x = 0; x < side; x++) rows[y * stride + 3 * x + c] = samples[x];
  }
  const grid = new Uint8ClampedArray(side * stride);
  for (let channel = 0; channel < stride; channel++) {
    const samples = line(y => rows[y * stride + channel]);
    for (let y = 0; y < side; y++) grid[y * stride + channel] = Math.round(samples[y]);
  }
  return grid;
}

function backgroundBitmap(grid, size) {
  const art = size * MASKABLE / LAYER, inset = (size - art) / 2;
  const axis = Array.from({ length: size }, (_, i) => {
    const pos = Math.max(0, Math.min(CELLS, (i + 0.5 - inset) * CELLS / art));
    const lo = Math.min(Math.floor(pos), CELLS - 1);
    return { lo, t: pos - lo };
  });
  const data = new Uint8Array(size * size * 4), stride = 3 * (CELLS + 1);
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const col = axis[x], row = axis[y], src = row.lo * stride + col.lo * 3, dst = 4 * (y * size + x);
    for (let c = 0; c < 3; c++) {
      const top = grid[src + c] * (1 - col.t) + grid[src + 3 + c] * col.t;
      const bottom = grid[src + stride + c] * (1 - col.t) + grid[src + stride + 3 + c] * col.t;
      data[dst + c] = Math.round(top * (1 - row.t) + bottom * row.t);
    }
    data[dst + 3] = 255;
  }
  return { width: size, height: size, data };
}

export function writeIconBackground(image, art, launchColour, write) {
  let colour;
  if (image.opaque) {
    const grid = colourGrid(art);
    if (!grid.every((value, i) => value === grid[i % 3])) {
      for (const [density, size] of Object.entries(sizes)) {
        write(`drawable-${density}/exact_icon_background.png`, encodePng(backgroundBitmap(grid, size)));
      }
      return '@drawable/exact_icon_background';
    }
    colour = rgb(grid);
  } else {
    colour = launchColour || rgb(image.data);
  }
  write('values/exact_icon.xml', `<resources>\n    <color name="exact_icon_background">${colour}</color>\n</resources>\n`);
  return '@color/exact_icon_background';
}
