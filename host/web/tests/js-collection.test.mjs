// The JS target's collection (host/web-js/list.js): its protocol and height index, evaluated with its imports bound to
// stand-ins.
import { test, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { imports } from './stand-ins.mjs';

const webJs = name => resolve(fileURLToPath(new URL(`../../web-js/${name}`, import.meta.url)));

// The collection protocol and index run unchanged; only DOM row construction
// and publication are stand-ins, as the runtime's imported hosts are below.
function collectionRuntime() {
  const Views = new Map();
  let next = 0;
  let mapCount = 0;
  const viewId = el => { const id = ++next; Views.set(id, el); return id; };
  const source = readFileSync(webJs('list.js'), 'utf8').replace(/^import .*;\n/gm, '').replace(/^export /gm, '');
  // Every name list.js imports is bound: these as given, the rest as functions that do nothing.
  const given = { viewId, Views, write() {}, writeItem() {}, Refusal: Error, Hosts: {}, Before: [], After: [], settled() {},
    Map: class extends Map { constructor(...args) { super(...args); mapCount++; } } };
  const names = [...new Set([...imports('list.js').flatMap(i => i.local ?? []), ...Object.keys(given)])];
  const { Collection, report, Lists, HeightPreview } = Function(...names, `${source}; return { Collection, report, Lists, HeightPreview };`)(
    ...names.map(n => n in given ? given[n] : function () {}));
  const make = (horizontal = false) => {
    const c = new Collection({}, { x: horizontal, pw: [], ph: [], est: 100 }, null);
    c.items = Array.from({ length: 100 }, (_, i) => i);
    c.index.replace(c.items.map(i => 'n:' + i));
    c.createRow = (position, key) => {
      const wrapper = { setAttribute() {}, contains: () => false }, view = viewId(wrapper);
      return { position, key, wrapper, view, root: view, published: [-1, -1], inner: [], item: {}, index: {} };
    };
    c.emit = () => {}; c.retire = () => {};
    c.margins = () => [0, 0]; c.scrollPadding = () => [0, 0];
    c.realize(false, {});
    Lists.set(c.view, c);
    return c;
  };
  const facts = c => ({ view: c.view, revision: c.revision, scroll_sequence: c.geometry?.scroll_sequence ?? 0,
    offset: 0, port_main: 600, port_cross: 320, cross: 320, padding: [0, 0], focus_view: null, interaction_view: null, measurements: [] });
  return { make, facts, send: f => report(null, f, {}), HeightPreview, maps: () => mapCount };
}

test('collection padding alone publishes a revision and rejects older same-token feedback', () => {
  for (const horizontal of [false, true]) {
    const { make, facts, send } = collectionRuntime(), c = make(horizontal);
    send(facts(c));
    const before = c.snapshot(), old = facts(c), padded = { ...old, padding: [0, 100] };
    send(padded);
    const after = c.snapshot();
    expect(after).toEqual({ ...before, revision: before.revision + 1 });
    send(old);
    expect(c.snapshot()).toEqual(after);
    expect([c.index.leading, c.index.trailing]).toEqual([0, 100]);
    send({ ...padded, revision: after.revision });
    expect(c.snapshot()).toEqual(after);
    c.intoView('n:99', 'start');
    expect(c.correction.offset).toBe(9500);
  }
});

test('collection padded overflow is rejected before heights, epochs, width and sibling pins mutate', () => {
  const max = 3.4028234663852886e38;
  for (const horizontal of [false, true]) for (let variant = 0; variant < 5; variant++) {
    const { make, facts, send } = collectionRuntime(), sibling = make(), c = make(horizontal);
    send({ ...facts(sibling), focus_view: sibling.mounted[0].view, interaction_view: sibling.mounted[1].view });
    send(facts(c));
    const row = c.mounted[0];
    const measurements = [{ view: row.view, epoch: row.epoch, size: 1e38 }];
    if (variant === 4) measurements.push({ view: c.mounted[1].view, epoch: c.mounted[1].epoch, size: 0 });
    send({ ...facts(c), measurements });
    if (variant === 4) c.est = 3e38;
    const before = JSON.stringify([c.snapshot(), c.geometry, c.index.h, c.index.epoch, sibling.snapshot(), sibling.geometry]);
    const f = { ...facts(c), padding: variant === 0 ? [max, 0] : [0, max], focus_view: row.view, interaction_view: row.view };
    if (variant === 2) f.cross++;
    if (variant === 3) { f.padding = [0, 2e38]; f.measurements = [{ view: row.view, epoch: row.epoch, size: 2e38 }]; }
    if (variant === 4) { f.cross++; f.padding = [0, 0]; }
    expect(() => send(f)).toThrow();
    expect(JSON.stringify([c.snapshot(), c.geometry, c.index.h, c.index.epoch, sibling.snapshot(), sibling.geometry])).toBe(before);
    if (variant === 3) {
      send({ ...facts(c), padding: [0, 3e38], measurements: [{ view: row.view, epoch: row.epoch, size: 1e37 }] });
      expect(c.index.extent).toBeLessThanOrEqual(max);
      expect(c.correction == null || Math.abs(c.correction.offset) <= max).toBe(true);
    }
  }
});
test('collection aggregate preview uses binary sums and checks both padding associations', () => {
  const initial = [3.855634828180998e37, 4.110381270879275e37, 1.2150761836965103e37, 1.4554140070018447e37];
  const next = [7.162362434933901e36, 1.743078113874182e37, 1.85932241262857e37, 3.2119624082087986e37];
  for (const horizontal of [false, true]) for (let variant = 0; variant < 3; variant++) {
    const { make, facts, send } = collectionRuntime(), sibling = make(), c = make(horizontal);
    send({ ...facts(sibling), focus_view: sibling.mounted[0].view, interaction_view: sibling.mounted[1].view });
    c.items = c.items.slice(0, variant === 0 ? 4 : 1); c.index.replace(c.items.map(i => 'n:' + i)); c.realize(false, {});
    const report = sizes => ({ ...facts(c), port_main: 1e38, offset: c.correction?.offset ?? 0,
      measurements: c.mounted.flatMap(m => sizes[m.position] === undefined ? [] : [{ view: m.view, epoch: m.epoch, size: sizes[m.position] }]) });
    send(report([])); send(report(variant === 0 ? initial : [1.3857361873091886e38]));
    c.followEnd = true; c.atEnd = true;
    const before = JSON.stringify([c.snapshot(), c.geometry, c.index.h, c.index.epoch, sibling.snapshot(), sibling.geometry]);
    const f = { ...report(variant === 0 ? next : []), port_main: 600, focus_view: c.mounted[0].view, interaction_view: c.mounted[0].view,
      padding: variant === 0 ? [0, 2.6497635485647948e38] : variant === 1 ? [1.383018962808564e37, 1.878785382795244e38] : [1.878785382795244e38, 1.383018962808564e37] };
    expect(() => send(f)).toThrow();
    expect(JSON.stringify([c.snapshot(), c.geometry, c.index.h, c.index.epoch, sibling.snapshot(), sibling.geometry])).toBe(before);
    send(report([]));
    expect(c.correction == null || Math.abs(c.correction.offset) <= 3.4028234663852886e38).toBe(true);
  }
});
test('collection normal changes skip the overlay; extreme preview touches only changed ancestors', () => {
  const { make, facts, send, HeightPreview, maps } = collectionRuntime(), c = make();
  c.items = Array.from({ length: 25000 }, (_, i) => i); c.index.replace(c.items.map(i => 'n:' + i));
  send(facts(c));
  const f = { ...facts(c), measurements: c.mounted.map(m => ({ view: m.view, epoch: m.epoch, size: c.index.h[m.position] })) };
  send(f);
  f.revision = c.revision;
  for (const noise of [0, 0.00003, 12, -100]) {
    const before = maps();
    expect(c.prepare({ ...f, measurements: f.measurements.map(m => ({ ...m, size: m.size + noise })) })).not.toBe(null);
    expect(maps() - before).toBe(1); // Existing byView map; no preview/ancestor map.
  }
  const total = c.index.total, preview = new HeightPreview(c.index.t);
  const changes = [[0, 1e38], [12000, 7.162362434933901e36], [24999, 0]];
  for (const [p, h] of changes) preview.set(p, h);
  expect(preview.sums.size).toBeLessThanOrEqual(changes.length * (Math.log2(c.index.t.base) + 1));
  expect(c.index.total).toBe(total);
  for (const [p, h] of changes) {
    expect(c.index.h[p]).toBe(100);
    c.index.setMeasured(c.index.order[p], c.index.token(c.index.order[p]), h);
  }
  expect(c.index.total).toBe(preview.total);
});
test('collection sufficient padded bound includes its boundary and bounds actual binary sums', () => {
  const max = 3.4028234663852886e38;
  for (const count of [0, 1, 2, 3, 257, 25000]) {
    const { make } = collectionRuntime(), c = make();
    c.items = Array.from({ length: count }, (_, i) => i); c.index.replace(c.items.map(i => 'n:' + i));
    const bound = max / (4 * c.index.t.base), bits = new DataView(new ArrayBuffer(8));
    bits.setFloat64(0, bound); bits.setBigUint64(0, bits.getBigUint64(0) + 1n);
    expect(c.index.paddedExtentDefinitelyFits(bound)).toBe(true);
    expect(c.index.paddedExtentDefinitelyFits(bits.getFloat64(0))).toBe(false);
    for (const invalid of [-1, NaN, Infinity]) expect(c.index.paddedExtentDefinitelyFits(invalid)).toBe(false);
    for (let p = 0; p < count; p++) c.index.setMeasured(c.index.order[p], c.index.token(c.index.order[p]), bound);
    expect(c.index.total).toBeLessThanOrEqual(c.index.t.base * bound);
    for (const [leading, trailing] of [[bound, bound], [bound / 3, bound], [bound, 0]]) {
      expect(c.index.total + leading + trailing).toBeLessThanOrEqual(max);
      expect(c.index.total + trailing + leading).toBeLessThanOrEqual(max);
    }
  }
});
test('collection normal fast path retains noise, revives zeros and discards old-width samples', () => {
  for (const horizontal of [false, true]) {
    const { make, facts, send, maps } = collectionRuntime(), c = make(horizontal);
    send(facts(c));
    const samples = size => c.mounted.map(m => ({ view: m.view, epoch: m.epoch, size }));
    const sample = size => ({ ...facts(c), measurements: samples(size) });
    send(sample(58)); send(sample(58.00003));
    expect(c.index.h[0]).toBe(58);
    send(sample(0));
    expect(c.index.h[0]).toBe(0);
    expect(c.zeros.size).toBeGreaterThan(0);
    const width = { ...sample(77), cross: facts(c).cross + 1, padding: [30, 70] }, before = maps();
    expect(c.prepare(width)).not.toBe(null);
    expect(maps() - before).toBe(1); // Pre-existing byView map; no sparse overlay.
    send(width);
    expect(c.index.h[0]).toBe(100); // Estimate, since samples describe the old width.
    expect(c.index.measured('n:0')).toBe(false);
    expect(c.zeros.size).toBe(0);
    expect([c.index.leading, c.index.trailing]).toEqual([30, 70]);
  }
});
