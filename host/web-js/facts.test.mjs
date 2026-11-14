import { expect, test } from 'bun:test';
import { copyFileSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { bundleHostModule } from './glue.mjs';

// LLP 1139.003 D2: a plan that reads only `exactViewport()` shares the
// session's one page reporter, so the agent's `prefer gpu-tier` reaches its
// `gpuTier`; with no prefer it is the agent's launch value, `high`.
test('a viewport-only plan reads the tier the agent prefers', async () => {
  const media = new Map();
  Object.assign(globalThis, {
    location: { search: '?agent' },
    document: { createElement: () => ({}), addEventListener() {}, documentElement: { style: { setProperty() {} } } },
    addEventListener() {}, matchMedia: query => ({ matches: false,
      addEventListener: (_, f) => media.set(query, f), removeEventListener: () => media.delete(query) }),
    innerWidth: 390, innerHeight: 844, devicePixelRatio: 2,
  });
  // As the build lays them out: the web host's navigation.js beside facts.js; the runtime stubbed to its data seam.
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-facts-'));
  try {
    copyFileSync(resolve(import.meta.dir, 'facts.js'), resolve(dir, 'facts.js'));
    writeFileSync(resolve(dir, 'navigation.js'), await bundleHostModule(resolve(import.meta.dir, '../..'), 'navigation.js'));
    writeFileSync(resolve(dir, 'rt.js'), `export const data = {}, Sounds = {}, Resources = [], commits = []; export const commit = (f, why) => { commits.push(why); f(); }; export function R() {} export const Refusal = Error; export const Kept = kind => 'exact.' + kind + '.'; export const say = () => {};`);
    const { viewport } = await import(resolve(dir, 'facts.js'));
    const { data, commits } = await import(resolve(dir, 'rt.js'));
    viewport({ v: [['gpuTier']], ratio: [['devicePixelRatio']], geometry: [['safeAreaTop', 'safeAreaBottom', 'segments'], { safeAreaTop: 'n', safeAreaBottom: 'n', segments: ['[]', { height: 'n', x: 'n', width: 'n', y: 'n' }] }] });
    const read = () => data.reserved.exactViewport(null, null, 'v')[0];
    expect(read()).toBe('high');
    const ratio = () => data.reserved.exactViewport(null, null, 'ratio')[0];
    expect(ratio()).toBe(2);
    globalThis.devicePixelRatio = 3;
    const before = commits.length;
    media.get('(resolution: 2dppx)')();
    expect(commits.length).toBe(before + 1);
    expect(ratio()).toBe(3);
    expect(media.has('(resolution: 2dppx)')).toBe(false);
    expect(media.has('(resolution: 3dppx)')).toBe(true);
    globalThis.devicePixelRatio = undefined;
    expect(ratio()).toBe(null);
    globalThis.exact.page.prefer({ 'gpu-tier': 'low' });
    expect(read()).toBe('low');
    expect(commits).toContain('viewport');
    document.body = { append() {} };
    document.createElement = () => ({ style: {} });
    globalThis.getComputedStyle = () => ({ paddingTop: '24px', paddingRight: '0px', paddingBottom: '34px', paddingLeft: '0px' });
    globalThis.visualViewport = { height: innerHeight };
    const geometry = () => data.reserved.exactViewport(null, null, 'geometry');
    expect(geometry()).toEqual([24, 34, [[844, 0, 390, 0]]]);
    globalThis.exact.fold.prefer({ cols: 2, rows: 1, gap: 10 });
    expect(geometry()).toEqual([24, 34, [[844, 0, 190, 0], [844, 200, 190, 0]]]);
    globalThis.exact.fold.prefer({ cols: 2, rows: 1, gap: 20 });
    expect(geometry()).toEqual([24, 34, [[844, 0, 185, 0], [844, 205, 185, 0]]]);
    expect(globalThis.exact.page.read()['gpu-tier']).toBe('low');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('canvas performance keeps unsupported metrics unavailable and refuses bad snapshots', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-performance-facts-'));
  try {
    copyFileSync(resolve(import.meta.dir, 'facts.js'), resolve(dir, 'facts.js'));
    writeFileSync(resolve(dir, 'navigation.js'), await bundleHostModule(resolve(import.meta.dir, '../..'), 'navigation.js'));
    writeFileSync(resolve(dir, 'rt.js'), `export const data = {}, Sounds = {}, Resources = [{name:'hud',source:'exactCanvasPerformance'}], answers = {};
export const Refusal = Error; export const Kept = kind => 'exact.' + kind + '.'; export const say = () => {}; export function R(r) { answers[r.name] = data.reserved[r.source](null,null,r.name); }
export function commit(f) { try { f(); return true; } catch { return false; } }`);
    const { canvasPerformance } = await import(resolve(dir, 'facts.js'));
    const { data, answers } = await import(resolve(dir, 'rt.js'));
    canvasPerformance({ hud: ['world', { frames: ['?', 'n'], gpuMs: ['?', 'n'] }] });
    const read = () => data.reserved.exactCanvasPerformance(null, null, 'hud');
    expect(read()).toEqual([null, null]);
    expect(exact.canvasPerformanceRequested('world')).toBe(true);
    expect(exact.canvasPerformanceRequested('unread')).toBe(false);
    exact.canvasPerformanceRecord('world', '{"frames":60}');
    expect(answers.hud).toEqual([60, null]);
    for (const bad of ['{"frames":"bad"}', '{"frames":1e999}', ' '.repeat(16385)]) {
      exact.canvasPerformanceRecord('world', bad);
      expect(read()).toEqual([60, null]);
      expect(Object.keys(exact.canvasPerformanceRefusals())).toEqual(['world']);
    }
    exact.canvasPerformanceRecord('unread', 'invalid');
    expect(Object.keys(exact.canvasPerformanceRefusals())).toEqual(['world']);
    exact.canvasPerformanceRecord('world', null);
    expect(read()).toEqual([null, null]);
    expect(exact.canvasPerformanceRefusals()).toEqual({});
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('kept surface readers seed launch, replace only on live publication and invalidate shape changes', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-kept-surface-')), saved = new Map();
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'localStorage');
  Object.defineProperty(globalThis, 'localStorage', {configurable:true, value:{get length(){return saved.size;}, key:i=>[...saved.keys()][i], getItem:k=>saved.get(k)??null, setItem:(k,v)=>saved.set(k,v), removeItem:k=>saved.delete(k)}});
  try {
    copyFileSync(resolve(import.meta.dir, 'facts.js'), resolve(dir, 'facts.js'));
    writeFileSync(resolve(dir, 'navigation.js'), await bundleHostModule(resolve(import.meta.dir, '../..'), 'navigation.js'));
    writeFileSync(resolve(dir, 'rt.js'), `export const data={}, Sounds={}, Resources=[{name:'hud',source:'exactSurface'}]; export const Kept=k=>'exact.'+k+'.', say=()=>{}, Refusal=Error;
export function R(r){data.reserved[r.source](null,null,r.name);} export function commit(f){try{f();return true;}catch{return false;}}`);
    const {surfaces} = await import(resolve(dir, 'facts.js')), {data} = await import(resolve(dir, 'rt.js'));
    const boot = (shape={count:'n'}, keep=true) => surfaces({hud:['world',shape,keep ? Buffer.from(JSON.stringify(shape)).toString('hex') : false]});
    const read = () => data.reserved.exactSurface(null,null,'hud');
    boot(); expect(read()).toEqual([0]);
    exact.surfaceRecord('world','{"count":7,"ignored":"not saved"}');
    expect(saved.size).toBe(1);
    boot(); expect(read()).toEqual([7]);
    exact.surfaceRecord('world','{"count":"bad"}'); expect(read()).toEqual([7]);
    exact.surfaceRecord('world','{"count":9}'); expect(read()).toEqual([9]);
    exact.surfaceRecord('world',null); expect(read()).toEqual([0]);
    boot(); expect(read()).toEqual([9]);
    exact.surfaceRecord('world','{"count":-0}'); boot(); expect(Object.is(read()[0], -0)).toBe(true);
    boot({renamed:'n'}); expect(read()).toEqual([0]); expect(saved.size).toBe(0);
    exact.surfaceRecord('world','{"renamed":3}');
    boot({renamed:'n'},false); expect(read()).toEqual([0]); expect(saved.size).toBe(0);
    boot({text:'s'});
    const max = 262144 - (5 + 5) - Buffer.byteLength(JSON.stringify({text:'s'})) - 5 - 5;
    exact.surfaceRecord('world',JSON.stringify({text:'\0'.repeat(max)}));
    expect(saved.size).toBe(1); boot({text:'s'}); expect(read()[0].length).toBe(max);
    exact.surfaceRecord('world',JSON.stringify({text:'x'.repeat(max + 1)}));
    expect(saved.size).toBe(0);
    // A canvas's `presented` is the host's fact of this launch: never kept.
    boot({count:'n',presented:'b'}); exact.surfaceRecord('world','{"count":4,"presented":true}');
    expect(read()).toEqual([4,true]); boot({count:'n',presented:'b'}); expect(read()).toEqual([4,false]);
    boot(); exact.surfaceRecord('world','{"count":8}'); data.keepSurfaceAnswers = false;
    boot(); expect(read()).toEqual([0]); expect(saved.size).toBe(0); data.keepSurfaceAnswers = true;
    Object.defineProperty(globalThis,'localStorage',{configurable:true,get(){throw Error('storage blocked');}});
    boot(); expect(read()).toEqual([0]);
  } finally {
    previous ? Object.defineProperty(globalThis,'localStorage',previous) : delete globalThis.localStorage;
    rmSync(dir,{recursive:true,force:true});
  }
});


test('delivery reads build-time versions even from a pre-baked plan', async () => {
  for (const [version, buildNumber] of [[undefined, undefined], ['2.3.4', '56'], ['', '']]) {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-delivery-facts-'));
    try {
      // The literals the build's Rolldown defines substitute, before either bundle runs.
      const source = await Bun.file(resolve(import.meta.dir, 'facts.js')).text();
      writeFileSync(resolve(dir, 'facts.js'), source.replace('globalThis.__exactVersion', JSON.stringify(version ?? '')).replace('globalThis.__exactBuildNumber', JSON.stringify(buildNumber ?? '')));
      writeFileSync(resolve(dir, 'navigation.js'), await bundleHostModule(resolve(import.meta.dir, '../..'), 'navigation.js'));
      writeFileSync(resolve(dir, 'rt.js'), `export const data = {}, Sounds = {}, Resources = []; export function commit() {} export function R() {} export const Refusal = Error, Kept = () => '', say = () => {};`);
      const { delivery, deliveryFacts } = await import(resolve(dir, 'facts.js'));
      const { data } = await import(resolve(dir, 'rt.js'));
      delivery({ build: [['buildNumber', 'version']], baked: [['version', 'seq', 'buildNumber'], ['old', 7, 'old']] });
      expect(data.reserved.exactDelivery(null, null, 'build')).toEqual([buildNumber ?? '', version ?? '']);
      expect(data.reserved.exactDelivery(null, null, 'baked')).toEqual([version ?? '', 7, buildNumber ?? '']);
      expect(deliveryFacts.version).toBe(version ?? '');
      expect(deliveryFacts.buildNumber).toBe(buildNumber ?? '');
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }
});

test('a render answers the bake default for frozen', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-frozen-default-'));
  const document = globalThis.document;
  try {
    delete globalThis.document;
    copyFileSync(resolve(import.meta.dir, 'facts.js'), resolve(dir, 'facts.js'));
    writeFileSync(resolve(dir, 'navigation.js'), await bundleHostModule(resolve(import.meta.dir, '../..'), 'navigation.js'));
    writeFileSync(resolve(dir, 'rt.js'), `export const data = {}, Sounds = {}, Resources = []; export function commit() {} export function R() {} export const Refusal = Error, Kept = () => '', say = () => {};`);
    const {page} = await import(resolve(dir, 'facts.js')), {data} = await import(resolve(dir, 'rt.js'));
    page({page:[['frozen']]});
    expect(data.reserved.exactPage(null, null, 'page')).toEqual([false]);
  } finally { globalThis.document = document; rmSync(dir, {recursive:true, force:true}); }
});
