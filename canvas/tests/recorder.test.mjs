// The TypeScript recorder against the shared cases (LLP 1056 §4): the same
// getters and throws Chrome gave, as the Rust recorder is held to them.
import { test, expect } from 'bun:test';
import { runCases } from './cases.mjs';
import { Recorder, parseColor, serializeColor } from '../recorder.js';

test('the shared cases hold for the TypeScript recorder', () => {
  const failed = runCases(() => new Recorder()).filter((r) => r.fail.length);
  expect(failed).toEqual([]);
});

test('colours serialise as Chrome serialises them', () => {
  const c = (s) => { const v = parseColor(s); return v === 'current' ? 'current' : v ? v.text ?? serializeColor(v) : 'none'; };
  expect(c('rgba(0,0,0,0.123456)')).toBe('rgba(0, 0, 0, 0.12)');
  expect(c('rgb(1 2 3 / 0.999)')).toBe('#010203');
  expect(c('hsl(120 50 50)')).toBe('#40bf40');
  expect(c('hsl(120, 50, 50)')).toBe('none');
  expect(c('#abcd')).toBe('rgba(170, 187, 204, 0.867)');
  // hwb() has no upper bound on whiteness or blackness: Chrome's rgb(170, 170, 170), 200 / 300.
  expect(c('hwb(0 200% 100%)')).toBe('#aaaaaa');
  expect(c('hwb(0 50% 150%)')).toBe('#404040');
  expect(c('hwb(200 120% 30%)')).toBe('#cccccc');
  expect(c('hsl(200 50% 150%)')).toBe('#ffffff');
  // A component written `none` reads back as `none` (Chrome 156).
  expect(c('oklch(0.7 0 none)')).toBe('oklch(0.7 0 none)');
  expect(c('lab(none 20 20)')).toBe('lab(none 20 20)');
  expect(c('oklch(0.7 0.1 120 / none)')).toBe('oklch(0.7 0.1 120 / none)');
  expect(c('color(srgb none 0.5 0.5)')).toBe('color(srgb none 0.5 0.5)');
});

// A list's records as [code, operands] (canvas/src/list.rs).
function records(bytes) {
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength), out = [];
  for (let at = 8; at + 8 <= v.byteLength;) {
    const code = v.getUint32(at, true), count = v.getUint32(at + 4, true), k = [];
    at += 8;
    for (let i = 0; i < count; i++, at += 8) k.push(v.getFloat64(at, true));
    out.push([code, k]);
  }
  return out;
}

test('arcTo near-collinear turns record a line to the corner', () => {
  for (const dy of [1e-7, 1e-4, 1e-2, 0.0244, 0.0245]) {
    for (const sign of [-1, 1]) for (const x2 of [0, 200]) for (const path2d of [false, true]) {
      const ctx = new Recorder(), path = path2d ? new Path2D() : ctx;
      path.moveTo(0, 50);
      path.arcTo(100, 50, x2, 50 + sign * dy, 5);
      if (path2d) ctx.stroke(path); else ctx.stroke();
      const lists = ctx._.take();
      expect(lists.length).toBe(1);
      const r = records(lists[0]);
      expect(r.every(([, k]) => k.every(Number.isFinite))).toBe(true);
      const lines = r.filter(([c]) => c === (path2d ? 81 : 42)).map(([, k]) => k);
      const curves = r.filter(([c]) => c === (path2d ? 83 : 44)).length;
      if (dy <= 0.0244) {
        expect(lines).toEqual([[100, 50]]);
        expect(curves).toBe(0);
      } else {
        expect(lines).not.toEqual([[100, 50]]);
        expect(curves).toBeGreaterThan(0);
      }
    }
  }
});

test('a display-p3 canvas records Display P3 bytes, as the Rust recorder does', () => {
  // canvas/tests/cases.rs `a_display_p3_canvas_records_display_p3_bytes`.
  const ctx = new Recorder();
  ctx._.env.p3 = true;
  ctx.fillStyle = 'red'; ctx.fillRect(0, 0, 1, 1);
  ctx.fillStyle = 'color(display-p3 1 0 0)'; ctx.fillRect(0, 0, 1, 1);
  ctx.putImageData(new ImageData(new Uint8ClampedArray([255, 0, 0, 255]), 1), 0, 0);
  expect(ctx.createImageData(1, 1).colorSpace).toBe('display-p3');
  const r = records(ctx._.take()[0]);
  expect(r.filter(([c]) => c === 10).map(([, k]) => k)).toEqual([[234, 51, 35, 1], [255, 0, 0, 1]]);
  const pixel = r.find(([c]) => c === 72)[1][4];
  expect([pixel >>> 24, (pixel >>> 16) & 255, (pixel >>> 8) & 255, pixel & 255]).toEqual([234, 51, 35, 255]);
});

test('a font-relative spacing follows a later font, as the Rust recorder does', () => {
  // canvas/tests/cases.rs `a_font_relative_spacing_follows_a_later_font`.
  const width = (ctx) => ctx.measureText('Hello World').width;
  const ctx = new Recorder(), plain = width(ctx);
  ctx.letterSpacing = '1em';
  const atTen = width(ctx) - plain;
  ctx.font = '20px serif';
  const other = new Recorder();
  other.font = '20px serif';
  expect(width(ctx) - width(other)).toBe(2 * atTen);
  expect(ctx.letterSpacing).toBe('1em');
  ctx.wordSpacing = '1ex';
  ctx.fillText('Hello World', 0, 0);
  const font = records(ctx._.take()[0]).find(([c]) => c === 60)[1];
  expect(font.slice(7, 9)).toEqual([20, 10]);
});

test('a number after Z ends the path there, as Chrome does', () => {
  // `Z` takes no numbers (SVG 2 §9.5.4); repeating it for the number never ended.
  const drawn = (d) => { const ctx = new Recorder(); ctx.stroke(new Path2D(d)); return ctx._.take(); };
  expect(drawn('M0 0 L10 0 Z 5')).toEqual(drawn('M0 0 L10 0 Z'));
  expect(drawn('M0 0 L10 0 z5 5')).toEqual(drawn('M0 0 L10 0 z'));
});
