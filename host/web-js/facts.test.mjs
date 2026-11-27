import { expect, test } from 'bun:test';
import { copyFileSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { bundleHostModule } from './glue.mjs';

// LLP 1139.003 D2: a plan that reads only `exactViewport()` shares the
// session's one page reporter, so the agent's `prefer gpu-tier` reaches its
// `gpuTier`; with no prefer it is the agent's launch value, `high`.
test('a viewport-only plan reads the tier the agent prefers', async () => {
  const media = new Map(), orientation = Object.assign(new EventTarget(), {type: "portrait-primary", angle: 0});
  const priorScreen = Object.getOwnPropertyDescriptor(globalThis, "screen");
  Object.assign(globalThis, {
    location: { search: '?agent', href: 'http://127.0.0.1/?agent' },
    document: { createElement: () => ({}), addEventListener() {}, documentElement: { style: { setProperty() {} } } },
    addEventListener() {}, matchMedia: query => ({ matches: false,
      addEventListener: (_, f) => media.set(query, f), removeEventListener: () => media.delete(query) }),
    screen: {orientation}, innerWidth: 390, innerHeight: 844, devicePixelRatio: 2,
  });
  // As the build lays them out: the web host's navigation.js beside facts.js; the runtime stubbed to its data seam.
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-facts-'));
  try {
    copyFileSync(resolve(import.meta.dir, 'facts.js'), resolve(dir, 'facts.js'));
    writeFileSync(resolve(dir, 'navigation.js'), await bundleHostModule(resolve(import.meta.dir, '../..'), 'navigation.js'));
    writeFileSync(resolve(dir, 'rt.js'), `export const deferForReplacement = () => false; export const data = {}, Sounds = {}, Resources = [], commits = []; export const commit = (f, why) => { commits.push(why); f(); }; export function R() {} export const Refusal = Error; export const Kept = kind => 'exact.' + kind + '.'; export const say = () => {};`);
    const { viewport, viewportFacts } = await import(resolve(dir, 'facts.js'));
    const { data, commits } = await import(resolve(dir, 'rt.js'));
    viewport({ facing: [['orientationType', 'orientationAngle']], v: [['gpuTier']], ratio: [['devicePixelRatio']], geometry: [['safeAreaTop', 'safeAreaBottom', 'segments'], { safeAreaTop: 'n', safeAreaBottom: 'n', segments: ['[]', { height: 'n', x: 'n', width: 'n', y: 'n' }] }] });
    const facing = () => data.reserved.exactViewport(null, null, 'facing');
    expect(facing()).toEqual(['portrait-primary', 0]);
    Object.assign(orientation, {type: 'portrait-secondary', angle: 180});
    const beforeTurn = commits.length;
    orientation.dispatchEvent(new Event('change'));
    expect(commits.slice(beforeTurn)).toEqual(['viewport']);
    expect(facing()).toEqual(['portrait-secondary', 180]);
    expect(viewportFacts().screenReader).toBe('unknown');
    for (const value of ['on', 'off', 'unknown']) {
      const before = commits.length;
      globalThis.exact.page.prefer({ 'screen-reader': value });
      expect(viewportFacts().screenReader).toBe(value);
      expect(commits.length).toBeGreaterThan(before);
    }
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
  } finally {
    if (priorScreen) Object.defineProperty(globalThis, "screen", priorScreen);
    else delete globalThis.screen;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('canvas performance keeps unsupported metrics unavailable and refuses bad snapshots', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-performance-facts-'));
  try {
    copyFileSync(resolve(import.meta.dir, 'facts.js'), resolve(dir, 'facts.js'));
    writeFileSync(resolve(dir, 'navigation.js'), await bundleHostModule(resolve(import.meta.dir, '../..'), 'navigation.js'));
    writeFileSync(resolve(dir, 'rt.js'), `export const deferForReplacement = () => false; export const data = {}, Sounds = {}, Resources = [{name:'hud',source:'exactCanvasPerformance'}], answers = {};
export const Refusal = Error; export const Kept = kind => 'exact.' + kind + '.'; export const say = () => {}; export function R(r) { answers[r.name] = data.reserved[r.source](null,null,r.name); }
export function commit(f) { try { f(); return true; } catch { return false; } }`);
    const { canvasPerformance } = await import(resolve(dir, 'facts.js'));
    const { data, answers, Resources } = await import(resolve(dir, 'rt.js'));
    canvasPerformance({ hud: ['world', { frames: ['?', 'n'], gpuMs: ['?', 'n'] }] });
    const read = () => data.reserved.exactCanvasPerformance(null, null, 'hud');
    expect(read()).toEqual([null, null]);
    expect(exact.canvasPerformanceRequested('world')).toBe(true);
    expect(exact.canvasPerformanceRequested('unread')).toBe(false);
    Resources[0].asleep = true;
    expect(exact.canvasPerformanceRequested('world')).toBe(false);
    Resources.push({ ...Resources[0], asleep: false });
    expect(exact.canvasPerformanceRequested('world')).toBe(true);
    Resources[1].gone = true;
    expect(exact.canvasPerformanceRequested('world')).toBe(false);
    Resources.pop();
    Resources[0].asleep = false;
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
    writeFileSync(resolve(dir, 'rt.js'), `export const deferForReplacement = () => false; export const data={}, Sounds={}, Resources=[{name:'hud',source:'exactSurface'}]; export const Kept=k=>'exact.'+k+'.', say=()=>{}, Refusal=Error;
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


test('delivery reads whether the page is a production build, even from a pre-baked plan', async () => {
  for (const production of [undefined, false, true]) {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-delivery-production-'));
    try {
      const source = await Bun.file(resolve(import.meta.dir, 'facts.js')).text();
      writeFileSync(resolve(dir, 'facts.js'), production === undefined ? source : source.replace('globalThis.__exactProduction', JSON.stringify(production)));
      writeFileSync(resolve(dir, 'navigation.js'), await bundleHostModule(resolve(import.meta.dir, '../..'), 'navigation.js'));
      writeFileSync(resolve(dir, 'rt.js'), `export const deferForReplacement = () => false; export const data = {}, Sounds = {}, Resources = []; export function commit() {} export function R() {} export const Refusal = Error, Kept = () => '', say = () => {};`);
      const { delivery, deliveryFacts } = await import(resolve(dir, 'facts.js'));
      const { data } = await import(resolve(dir, 'rt.js'));
      // A plan baked by a development build answers false; the page's own build decides.
      delivery({ build: [['production']], baked: [['production', 'seq'], [!production, 7]] });
      expect(data.reserved.exactDelivery(null, null, 'build')).toEqual([production ?? false]);
      expect(data.reserved.exactDelivery(null, null, 'baked')).toEqual([production ?? false, 7]);
      expect(deliveryFacts.production).toBe(production ?? false);
    } finally { rmSync(dir, { recursive: true, force: true }); }
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
      writeFileSync(resolve(dir, 'rt.js'), `export const deferForReplacement = () => false; export const data = {}, Sounds = {}, Resources = []; export function commit() {} export function R() {} export const Refusal = Error, Kept = () => '', say = () => {};`);
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
    writeFileSync(resolve(dir, 'rt.js'), `export const deferForReplacement = () => false; export const data = {}, Sounds = {}, Resources = []; export function commit() {} export function R() {} export const Refusal = Error, Kept = () => '', say = () => {};`);
    const {page} = await import(resolve(dir, 'facts.js')), {data} = await import(resolve(dir, 'rt.js'));
    page({page:[['frozen']]});
    expect(data.reserved.exactPage(null, null, 'page')).toEqual([false]);
  } finally { globalThis.document = document; rmSync(dir, {recursive:true, force:true}); }
});

test('the geometry facts are unavailable where nothing is laid out, the native quantities none', async () => {
  // LLP 1123 D6: with no element of that id laid out a reader is answered, not failed, with what it declares; the
  // native chain and the unsettled list are unavailable on the web
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-geometry-facts-'));
  try {
    writeFileSync(resolve(dir, 'facts.js'), await Bun.file(resolve(import.meta.dir, 'facts.js')).text());
    writeFileSync(resolve(dir, 'navigation.js'), await bundleHostModule(resolve(import.meta.dir, '../..'), 'navigation.js'));
    writeFileSync(resolve(dir, 'rt.js'), `export const deferForReplacement = () => false; export const data = {}, Sounds = {}, Resources = []; export function commit() {} export function R() {} export const Refusal = Error, Kept = () => '', say = () => {};`);
    const { geometry, nativeChain, safeAreaUnsettled } = await import(resolve(dir, 'facts.js'));
    const { data } = await import(resolve(dir, 'rt.js'));
    geometry({ g: [['available', 'frame', 'observations']] });
    nativeChain({ c: [['available']] });
    safeAreaUnsettled({ u: [['available', 'count', 'ids']] });
    expect(data.reserved.exactGeometry(null, ['hero'], 'g')).toEqual([false, null, null]);
    expect(data.reserved.exactNativeChain(null, ['hero'], 'c')).toEqual([false]);
    expect(data.reserved.exactSafeAreaUnsettled(null, [], 'u')).toEqual([false, 0, []]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test("a geometry fact is the browser's layout of the element, answered again as the layout moves it", async () => {
  // LLP 1123 D6: the element's size and frame in the document, the viewport or a node's border box, the other
  // quantities none; after a commit that moves it a commit of its own answers again at the next generation, and at
  // the bound of eight such commits in a row the answers hold until another cause
  const rects = { hero: { left: 10, top: 120, width: 200, height: 80 }, card: { left: 0, top: 100, width: 300, height: 400 } };
  const element = (id) => rects[id] && { getClientRects: () => [rects[id]], getBoundingClientRect: () => rects[id] };
  const prior = globalThis.document;
  Object.assign(globalThis, { document: { getElementById: element }, scrollX: 0, scrollY: 40 });
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-geometry-layout-'));
  try {
    writeFileSync(resolve(dir, 'facts.js'), await Bun.file(resolve(import.meta.dir, 'facts.js')).text());
    writeFileSync(resolve(dir, 'navigation.js'), await bundleHostModule(resolve(import.meta.dir, '../..'), 'navigation.js'));
    writeFileSync(resolve(dir, 'rt.js'), `export const deferForReplacement = () => false; export const data = {}, Sounds = {}, After = [], commits = [], said = [];
      export const Resources = [{ name: 'g', source: 'exactGeometry', asked: ['hero'] }, { name: 'n', source: 'exactGeometry', asked: ['hero', 'node', 'card'] }];
      export function R(r) { commits.at(-1).read.push(data.reserved.exactGeometry(null, r.asked, r.name)); }
      export function commit(f, why) { commits.push({ why, read: [] }); f(); for (const a of After) a(); }
      export const Refusal = Error, Kept = () => '', say = (m) => said.push(m);`);
    const { geometry } = await import(resolve(dir, 'facts.js'));
    const { data, After, commits, said } = await import(resolve(dir, 'rt.js'));
    const rect = { x: 'n', y: 'n', width: 'n', height: 'n' };
    const shape = { available: 'b', generation: 'n', size: ['?', { width: 'n', height: 'n' }], frame: ['?', rect], observations: ['?', 'n'], keyboard: ['?', 'n'] };
    geometry({ g: [['available', 'generation', 'size', 'frame', 'observations', 'keyboard'], shape], n: [['available', 'generation', 'size', 'frame', 'observations', 'keyboard'], shape] });
    const read = (name, args) => data.reserved.exactGeometry(null, args, name);
    expect(read('g', ['hero'])).toEqual([true, 0, [200, 80], [10, 160, 200, 80], null, null]);
    expect(read('n', ['hero', 'node', 'card'])).toEqual([true, 0, [200, 80], [10, 20, 200, 80], null, null]);
    expect(read('g', ['hero', 'window'])[3]).toEqual([10, 120, 200, 80]);
    expect(read('g', ['gone'])).toEqual([false, 0, null, null, null, null]);
    read('g', ['hero']);
    // A commit that moves the element: one of its own answers again at the next generation, and then it rests
    rects.hero = { left: 10, top: 130, width: 200, height: 80 };
    for (const a of After) a();
    expect(commits.map(c => c.why)).toEqual(['geometry']);
    expect(commits[0].read).toEqual([[true, 1, [200, 80], [10, 170, 200, 80], null, null], [true, 1, [200, 80], [10, 30, 200, 80], null, null]]);
    // An answer that moves its own layout each time: eight in a row, then the answers hold
    commits.length = 0;
    let top = 130;
    data.reserved.exactGeometry = ((read) => (s, args, name) => { rects.hero = { ...rects.hero, top: ++top }; return read(s, args, name); })(data.reserved.exactGeometry);
    rects.hero = { ...rects.hero, top: ++top };
    for (const a of After) a();
    expect(commits.length).toBe(8);
    expect(said.some(m => m.startsWith('geometry-feedback'))).toBe(true);
  } finally { globalThis.document = prior; rmSync(dir, { recursive: true, force: true }); }
});

test("each geometry reader's answer is its own, refreshed only for a move, and a transformed node's space is none", async () => {
  // LLP 1123 D6: two instances of one declaration reading two elements keep their own snapshots, so moving the second
  // refreshes; a refresh's new generation alone moves nothing, so a quiet layout makes no further commit; and a frame in
  // a transformed node's space is none
  const rects = { hero: { left: 10, top: 120, width: 200, height: 80 }, card: { left: 0, top: 100, width: 300, height: 400 } };
  const style = {};
  const element = (id) => rects[id] && { nodeType: 1, id, getClientRects: () => [rects[id]], getBoundingClientRect: () => rects[id] };
  const prior = { document: globalThis.document, getComputedStyle: globalThis.getComputedStyle };
  Object.assign(globalThis, { document: { getElementById: element }, scrollX: 0, scrollY: 0, getComputedStyle: (el) => style[el.id] ?? {} });
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-geometry-instances-'));
  try {
    writeFileSync(resolve(dir, 'facts.js'), await Bun.file(resolve(import.meta.dir, 'facts.js')).text());
    writeFileSync(resolve(dir, 'navigation.js'), await bundleHostModule(resolve(import.meta.dir, '../..'), 'navigation.js'));
    writeFileSync(resolve(dir, 'rt.js'), `export const deferForReplacement = () => false; export const data = {}, Sounds = {}, After = [], commits = [];
      export const Resources = [{ name: 'g', source: 'exactGeometry', asked: ['hero'] }, { name: 'g', source: 'exactGeometry', asked: ['card'] }];
      export function R(r) { commits.at(-1).read.push(data.reserved.exactGeometry(null, r.asked, r.name)); }
      export function commit(f, why) { commits.push({ why, read: [] }); f(); for (const a of After) a(); }
      export const Refusal = Error, Kept = () => '', say = () => {};`);
    const { geometry } = await import(resolve(dir, 'facts.js'));
    const { data, After, commits } = await import(resolve(dir, 'rt.js'));
    const rect = { x: 'n', y: 'n', width: 'n', height: 'n' };
    geometry({ g: [['generation', 'frame'], { generation: 'n', frame: ['?', rect] }] });
    const read = (args) => data.reserved.exactGeometry(null, args, 'g');
    expect(read(['hero'])).toEqual([0, [10, 120, 200, 80]]);
    expect(read(['card'])).toEqual([0, [0, 100, 300, 400]]);
    // The second instance's element moves: one commit answers both at generation 1, and then nothing more
    rects.card = { left: 0, top: 110, width: 300, height: 400 };
    for (const a of After) a();
    expect(commits.map((c) => c.why)).toEqual(['geometry']);
    expect(commits[0].read).toEqual([[1, [10, 120, 200, 80]], [1, [0, 110, 300, 400]]]);
    // A node's space under a transform cannot be recovered from rectangles
    style.card = { transform: 'matrix(2, 0, 0, 2, 0, 0)' };
    expect(read(['hero', 'node', 'card'])).toEqual([1, null]);
  } finally { Object.assign(globalThis, prior); rmSync(dir, { recursive: true, force: true }); }
});

test("a geometry reader that reads other arguments, or has gone, keeps no snapshot of what it read", async () => {
  // LLP 1123 D6: after each commit only what a live instance reads now is compared, so an element a reader has left, or
  // the element of a reader disposed of, moves nothing
  const rects = { old: { left: 0, top: 0, width: 10, height: 10 }, current: { left: 0, top: 50, width: 10, height: 10 } };
  const element = (id) => rects[id] && { nodeType: 1, id, getClientRects: () => [rects[id]], getBoundingClientRect: () => rects[id] };
  const prior = { document: globalThis.document, getComputedStyle: globalThis.getComputedStyle };
  Object.assign(globalThis, { document: { getElementById: element }, scrollX: 0, scrollY: 0, getComputedStyle: () => ({}) });
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-geometry-live-'));
  try {
    writeFileSync(resolve(dir, 'facts.js'), await Bun.file(resolve(import.meta.dir, 'facts.js')).text());
    writeFileSync(resolve(dir, 'navigation.js'), await bundleHostModule(resolve(import.meta.dir, '../..'), 'navigation.js'));
    writeFileSync(resolve(dir, 'rt.js'), `export const deferForReplacement = () => false; export const data = {}, Sounds = {}, After = [], commits = [];
      export const Resources = [{ name: 'g', source: 'exactGeometry', asked: ['old'] }];
      export function R(r) { commits.at(-1).read.push(data.reserved.exactGeometry(null, r.asked, r.name)); }
      export function commit(f, why) { commits.push({ why, read: [] }); f(); for (const a of After) a(); }
      export const Refusal = Error, Kept = () => '', say = () => {};`);
    const { geometry } = await import(resolve(dir, 'facts.js'));
    const { data, After, commits, Resources } = await import(resolve(dir, 'rt.js'));
    geometry({ g: [['frame'], { frame: ['?', { x: 'n', y: 'n', width: 'n', height: 'n' }] }] });
    const read = (args) => data.reserved.exactGeometry(null, args, 'g');
    read(['old']);
    // The reader now reads another element: the one it left moves, and nothing is answered again
    Resources[0].asked = ['current'];
    read(['current']);
    for (const a of After) a();
    rects.old = { ...rects.old, top: 5 };
    for (const a of After) a();
    expect(commits).toEqual([]);
    // Disposed of, its element moving answers nothing either
    Resources[0].gone = true;
    rects.current = { ...rects.current, top: 60 };
    for (const a of After) a();
    expect(commits).toEqual([]);
  } finally { Object.assign(globalThis, prior); rmSync(dir, { recursive: true, force: true }); }
});
