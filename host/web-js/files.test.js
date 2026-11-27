import { test, expect, mock } from 'bun:test';
import { readFileSync } from 'node:fs';
import { loadOnce } from '../web/load-once.js';
import { glueDeclarations, glueNode } from '../web/tests/glue-fixture.mjs';

mock.module('./rt.js', () => ({
  journal: [], clock: { now: 0, agent: true }, nextTicket: () => 1,
  inflight: { n: 0 }, Hosts: {}, OnHooks: {}, viewId: () => 1, Views: new Map(), data: {}, painted: () => Promise.resolve(), pressFeedback: () => Promise.resolve(),
}));
mock.module('./pointer.js', () => ({ record: () => {} }));
const { inertAncestor } = await import('../web/navigation/layers.js');
const { liveById } = await import('../web/navigation/routes.js');
mock.module('./navigation.js', () => ({ inertAncestor, liveById, reportPlace: () => '' }));
// The real admission over the grant set the Rust parser writes for
// `fs.read "app:/data/a b"` (runner/src/grants.rs `normalized_json`, pinned
// byte for byte in save_file.rs's test), so a share's path check here is the
// runner's.
const { coversPath, createGrantSet, grantError } = await import('../web/grant-admission.js');
const GRANTS = createGrantSet(JSON.parse('{"version":1,"entries":[[1,"fs.read \\"app:/data/a b\\"",["fs-read","app:","data","a b"],null]],"error":null,"seal":"ed67c8ab1966767e"}'));
mock.module('./admission.js', () => ({ appGrantSet: () => GRANTS, coversPath }));

const rt = await import('./rt.js');
const { accepts, deliveredType, tokens, holds } = await import('./files.js');

test('held document and export validators accept fully qualified Windows paths', () => {
  const source = readFileSync(new URL('./files.js', import.meta.url), 'utf8');
  const helpers = glueDeclarations(source, ['documentPaths', 'isAbsolutePath']);
  const checker = (message, multiple = false) => {
    const assignment = glueNode(source, node => node.type === 'AssignmentExpression'
      && node.left.type === 'MemberExpression' && node.left.property.name === 'check'
      && source.slice(node.start, node.end).includes(message) && node);
    return new Function('Holds', 'multiple', helpers + assignment + '; return Holds[0].check;')([{}], multiple);
  };
  for (const check of [checker('type an absolute path'), checker('type an absolute file path')]) {
    for (const path of [String.raw`C:\temp\selected note.md`, 'C:/temp/selected note.md', String.raw`\\server\share\note.md`, String.raw`\\?\C:\temp\note.md`, String.raw`\\?\UNC\server\share\note.md`, '/tmp/selected note.md']) {
      expect(check(path), path).toBeNull();
      expect(check(path + '\n'), path).toBeNull();
    }
    for (const path of [String.raw`C:note.md`, String.raw`\note.md`, String.raw`1:\note.md`, 'note.md', ''])
      expect(check(path), path).toContain('absolute');
  }
  const many = 'C:/one note.md\nC:/two notes.md';
  expect(checker('type an absolute path')(many)).toBe('one path, not 2');
  expect(checker('type an absolute path', true)(many)).toBeNull();
  expect(checker('type an absolute file path')('C:/folder/')).toContain('absolute file path');
});

test('the web pickers ignore the Mac message and keep the shared options', () => {
  const el = { closest: () => null, getAttribute: () => null, isConnected: true };
  const previous = globalThis.document;
  globalThis.document = { getElementById: () => el, contains: n => n === el };
  try {
    for (const [name, args, expected] of [
      ['showDirectoryPicker', ['folder', null, 'Choose a project'], { id: 'folder' }],
      ['showOpenFilePicker', ['open', true, 'downloads', 'Choose files'], { id: 'open', multiple: true, startIn: 'downloads' }],
      ['showSaveFilePicker', ['save', 'a.png', null, 'Save a copy'], { id: 'save', suggestedName: 'a.png' }],
    ]) {
      rt.Hosts[name](...args);
      expect(holds().at(-1).device.args).toEqual(expected);
    }
    rt.Hosts.showDirectoryPicker('folder', null, 42);
    expect(rt.journal.at(-1)).toContain('showDirectoryPicker: refused:');
  } finally {
    el.isConnected = false;
    holds();
    if (previous === undefined) delete globalThis.document;
    else globalThis.document = previous;
  }
});

test('file input accept admits any type only when unrestricted', () => {
  const absent = tokens({ getAttribute: () => null });
  const empty = tokens({ getAttribute: () => '' });
  const mixed = tokens({ getAttribute: () => 'image/jpeg, */*' });
  expect(absent).toEqual([]);
  expect(empty).toEqual([]);
  expect(mixed).toEqual(['image/jpeg', '*/*']);
  for (const name of ['archive.xyz', 'notes.txt']) {
    expect(accepts(absent, name, 'application/octet-stream')).toBe(true);
    expect(accepts(empty, name, 'application/octet-stream')).toBe(true);
    expect(accepts(['*/*'], name, 'application/octet-stream')).toBe(true);
    expect(accepts(mixed, name, 'application/octet-stream')).toBe(true);
  }
  expect(accepts(['image/jpeg', '.txt'], 'archive.xyz', 'application/octet-stream')).toBe(false);
  expect(accepts(['image/jpeg', '.txt'], 'notes.txt', 'text/plain')).toBe(true);
});

test('a wildcard in a mixed accept preserves the original HEIC type', () => {
  expect(deliveredType('image/heic', ['image/jpeg', '*/*'])).toBe('image/heic');
  expect(deliveredType('image/heif', ['image/jpeg', '*/*'])).toBe('image/heif');
  expect(deliveredType('image/heic', ['image/jpeg'])).toBe('image/jpeg');
});

// LLP 1125 D12: the JS target's share holds its files under the agent, and
// refuses a picture of an element, a file outside `fs.read`, a path that is
// no app file, and nothing to share, in the runner's words (share.rs).
test('a share holds its files under the agent and refuses what the web cannot share', () => {
  rt.journal.length = 0;
  globalThis.Element ??= class {}; // no DOM: no pressed node to anchor to
  expect(grantError(GRANTS)).toBeFalsy();
  rt.Hosts.share('T', null, null, ['app:/data/a b/n.txt']);
  expect(holds().at(-1).device).toEqual({ capability: 'share', args: { title: 'T', text: null, url: null, files: ['app:/data/a b/n.txt'], anchor: null } });
  rt.Hosts.share(null, 'x', null, undefined);
  expect(holds().at(-1).device.args.files).toEqual([]);
  rt.Hosts.share(null, 'x', null, ['#card']);
  rt.Hosts.share(null, null, null, ['app:/data/a/n.txt']);
  rt.Hosts.share(null, null, null, ['/etc/passwd']);
  rt.Hosts.share(null, null, null, []);
  expect(rt.journal.filter(l => l.includes('refused')).map(l => l.replace(/^t=\d+ /, ''))).toEqual([
    'share: refused: a picture of an element is not available on the web',
    "share: refused: app:/data/a/n.txt is outside the app's fs.read grants",
    "share: refused: /etc/passwd is neither an app:/ file nor an element's #id",
    'share: refused: needs text=, url= or files=',
  ]);
});

test.each(['js', 'wasm'])('%s journals a file share once when its adapter loads or refuses', async target => {
  const js = readFileSync(new URL('./files.js', import.meta.url), 'utf8');
  const wasm = readFileSync(new URL('../web/wasm/commit.js', import.meta.url), 'utf8');
  const assignment = glueNode(js, node => node.type === 'AssignmentExpression'
    && node.left.object?.name === 'Hosts' && node.left.property?.name === 'share' && node);
  const command = glueNode(wasm, node => node.type === 'IfStatement'
    && node.test.type === 'BinaryExpression' && node.test.left.object?.name === 'op'
    && node.test.left.property?.name === 'name' && node.test.right.value === 'share' && node.consequent);
  const files = ['app:/data/a b/n.txt'];
  for (const outcome of ['load failure', 'adapter failure', 'shared', 'dismissed']) {
    const lines = [], calls = [], pending = new Set();
    const load = () => outcome === 'load failure' ? Promise.reject(new TypeError('load failed')) : Promise.resolve({
      share: async (details, paths) => {
        calls.push({ details, paths });
        if (outcome === 'adapter failure') throw Object.assign(new Error('refused'), { name: 'NotAllowedError' });
        return `share: ${outcome}`;
      },
    });
    const bindings = {
      Hosts: {}, clock: { agent: false }, Element: class {}, navigator: { share() {} },
      say: line => lines.push(line), log: line => lines.push(line),
      coversPath, appGrantSet: () => GRANTS, glue: load, picker: load,
      counted: p => { pending.add(p); p.then(() => pending.delete(p), () => pending.delete(p)); },
      inflight: pending, agentMode: false, writeIn: value => value, readOut: value => value,
      host: { wasm: { exact_command: () => JSON.stringify({ present: true, files: files.map(path => ({ path })) }) } },
      op: { args: ['Title', 'Text', null, files] },
    };
    new Function(...Object.keys(bindings), target === 'js'
      ? assignment + '; Hosts.share(...op.args);' : command)(...Object.values(bindings));
    expect(pending.size, `${target}: ${outcome}`).toBe(1);
    await Promise.allSettled([...pending]);
    expect(lines, `${target}: ${outcome}`).toEqual([outcome === 'load failure'
      ? 'share: refused: TypeError' : outcome === 'adapter failure' ? 'share: refused: NotAllowedError' : `share: ${outcome}`]);
    expect(pending.size, `${target}: ${outcome}`).toBe(0);
    expect(calls, `${target}: ${outcome}`).toEqual(outcome === 'load failure' ? [] : [{
      details: { title: 'Title', text: 'Text', url: null }, paths: files.map(path => ({ path })),
    }]);
  }
});

test.each(['js', 'wasm'])('%s retries a refused picker load and shares with the cached adapter', async target => {
  const js = readFileSync(new URL('./files.js', import.meta.url), 'utf8');
  const wasm = readFileSync(new URL('../web/wasm/commit.js', import.meta.url), 'utf8');
  const glueSource = readFileSync(new URL('../web/glue.js', import.meta.url), 'utf8');
  const assignment = glueNode(js, node => node.type === 'AssignmentExpression'
    && node.left.object?.name === 'Hosts' && node.left.property?.name === 'share' && node);
  const command = glueNode(wasm, node => node.type === 'IfStatement'
    && node.test.type === 'BinaryExpression' && node.test.left.object?.name === 'op'
    && node.test.left.property?.name === 'name' && node.test.right.value === 'share' && node.consequent);
  const lines = [], shared = [], pending = new Set(), files = ['app:/data/a b/n.txt'];
  let attempts = 0, installs = 0;
  const adapter = { at() { return this; }, share: async (details, paths) => { shared.push({ details, paths }); return 'share: shared'; } };
  const install = () => { installs++; return adapter; };
  const load = () => ++attempts === 1 ? Promise.reject(new TypeError('network failed')) : Promise.resolve(install);
  const bindings = {
    Hosts: {}, clock: { agent: false }, Element: class {}, navigator: { share() {} },
    say: line => lines.push(line), log: line => lines.push(line),
    coversPath, appGrantSet: () => GRANTS, loadAfterPaint: load,
    loadOnce, loadPicker: load, x: () => ({ picker: install }), data: { appId: 'retry' },
    counted: p => { pending.add(p); p.then(() => pending.delete(p), () => pending.delete(p)); },
    inflight: pending, agentMode: false, writeIn: value => value, readOut: value => value,
    host: { incarnation: 1, wasm: { exact_command: () => JSON.stringify({ present: true, files: files.map(path => ({ path })) }) } },
    op: { args: ['Title', 'Text', null, files] },
  };
  const loader = target === 'js' ? glueDeclarations(js, ['loads', 'glue'])
    .replace("import(new URL('./picker-glue.js', import.meta.url).href)", 'loadPicker()') : glueDeclarations(glueSource, ['picker']);
  const run = new Function(...Object.keys(bindings), loader + (target === 'js'
    ? assignment + '; return () => Hosts.share(...op.args);' : '; return () => {' + command + '};'))(...Object.values(bindings));
  run(); run();
  await Promise.allSettled([...pending]);
  expect(lines).toEqual(['share: refused: TypeError', 'share: refused: TypeError']);
  expect(attempts).toBe(1);
  run();
  await Promise.allSettled([...pending]);
  run();
  await Promise.allSettled([...pending]);
  expect(lines).toEqual(['share: refused: TypeError', 'share: refused: TypeError', 'share: shared', 'share: shared']);
  expect(attempts).toBe(2);
  expect(installs).toBe(1);
  expect(shared).toEqual(Array.from({ length: 2 }, () => ({
    details: { title: 'Title', text: 'Text', url: null }, paths: files.map(path => ({ path })),
  })));
  expect(pending.size).toBe(0);
});

test.each(['js', 'wasm'])('%s retries document glue after a refused load or install', async target => {
  const js = readFileSync(new URL('./files.js', import.meta.url), 'utf8');
  const wasm = readFileSync(new URL('../web/glue.js', import.meta.url), 'utf8');
  for (const failure of ['load', 'install']) {
    let attempts = 0, installs = 0;
    const refused = new TypeError('unavailable'), adapter = { at() { return this; } };
    const documents = { keeping() {}, install() { if (++installs === 1 && failure === 'install') throw refused; return adapter; } };
    const load = () => ++attempts === 1 && failure === 'load' ? Promise.reject(refused) : Promise.resolve(documents);
    const bindings = { x: () => ({ documents }), data: { appId: 'retry' }, appGrantSet: () => GRANTS,
      loadOnce, loadDocuments: load, loadAfterPaint: load, globalThis: { exact: { compat: { inputs: { app: 'retry' } } } },
      host: { incarnation: 1 }, log() {}, say() {} };
    const source = target === 'js' ? glueDeclarations(js, ['loads', 'docs'])
      .replace("import(new URL('./documents-glue.js', import.meta.url).href)", 'loadDocuments()') : glueDeclarations(wasm, ['documentsGlue']);
    const run = new Function(...Object.keys(bindings), source + '; return ' + (target === 'js' ? 'docs' : 'documentsGlue'))(...Object.values(bindings));
    const first = run(), concurrent = run();
    expect(await Promise.allSettled([first, concurrent])).toEqual([
      { status: 'rejected', reason: refused }, { status: 'rejected', reason: refused },
    ]);
    expect(attempts).toBe(1);
    expect(await run()).toBe(adapter);
    expect(await run()).toBe(adapter);
    expect(attempts).toBe(2);
    expect(installs).toBe(failure === 'load' ? 1 : 2);
  }
});

test('a memoized load shares its promise and forgets a synchronous or asynchronous refusal', async () => {
  for (const failure of ['throw', 'reject']) {
    const cache = {}, refused = new TypeError('network failed'), adapter = {};
    let attempts = 0;
    const load = () => { if (++attempts === 1) { if (failure === 'throw') throw refused; return Promise.reject(refused); } return adapter; };
    const first = loadOnce(cache, 'module', load);
    expect(loadOnce(cache, 'module', load)).toBe(first);
    expect(await Promise.allSettled([first])).toEqual([{ status: 'rejected', reason: refused }]);
    expect(cache.module).toBeNull();
    const next = loadOnce(cache, 'module', load);
    expect(await next).toBe(adapter);
    expect(loadOnce(cache, 'module', load)).toBe(next);
    expect(attempts).toBe(2);
  }
  const cache = {}, replacement = Promise.resolve('newer');
  let reject;
  const first = loadOnce(cache, 'module', () => new Promise((_, no) => { reject = no; }));
  await Promise.resolve();
  cache.module = replacement;
  reject(new Error('old load'));
  await Promise.allSettled([first]);
  expect(cache.module).toBe(replacement);
});

test('a pieces editor retries its load and retains the installed module', async () => {
  const source = readFileSync(new URL('./pieces.js', import.meta.url), 'utf8');
  let attempts = 0, installed = 0;
  const pending = [], lines = [], inflight = { n: 0 }, el = { isConnected: true };
  const bindings = {
    loadOnce: (...args) => { const p = loadOnce(...args); pending.push(p); return p; },
    load: () => ++attempts === 1 ? Promise.reject(new TypeError('network')) : Promise.resolve(),
    requestAnimationFrame() {}, globalThis: { exact: { installMarkupEditor: () => installed++ } },
    Owner: null, onEnd() {}, inflight, Hosts: {}, say: line => lines.push(line), viewId: () => 1,
  };
  const mde = new Function(...Object.keys(bindings), glueDeclarations(source, [source.includes('const loads =') ? 'loads' : 'Editor', 'mde'])
    .replace("import('./markup-editor.js')", 'load()') + '; return mde;')(...Object.values(bindings));
  const settle = async () => { await Promise.allSettled(pending.splice(0)); for (let i = 0; inflight.n && i < 10; i++) await Promise.resolve(); expect(inflight.n).toBe(0); };
  mde(el); mde(el); await settle();
  expect(lines).toEqual(['markup editor: network', 'markup editor: network']);
  expect(attempts).toBe(1);
  const refused = bindings.Hosts.format(1, 'bold');
  expect(refused).toBeInstanceOf(Promise);
  await expect(refused).rejects.toThrow('markup editor: unavailable');
  mde(el); await settle();
  mde(el); await settle();
  expect(attempts).toBe(2);
  expect(installed).toBe(2);
});

test.each(['fs', 'sqlite', 'documents'])('storage retries a refused %s adapter in its owner checkpoint', async kind => {
  const source = readFileSync(new URL('../web/storage.js', import.meta.url), 'utf8');
  let attempts = 0, factories = 0;
  const owner = {}, backend = { readFile: () => 'contents', open: () => ({ close: async () => {} }) };
  const factory = () => { factories++; return backend; };
  const documents = { keeping() {}, files: factory };
  const bindings = { loadOnce, knownFolderPath: () => false, agentReads: [], directories: [],
    loadModule: () => ++attempts === 1 ? Promise.reject(new TypeError('network')) : Promise.resolve({ createFileSystem: factory, createSqlite: factory }),
    globalThis: { exact: { documents } }, window: {} };
  const create = new Function(...Object.keys(bindings), glueDeclarations(source, ['fileFactory', 'sqliteFactory', 'toldAgent', 'createStorage'])
    .replace("import('./storage-fs.js')", 'loadModule()').replace("import('./storage-sqlite.js')", 'loadModule()')
    .replace("import('./documents-glue.js')", 'loadModule()') + '; return createStorage;')(...Object.values(bindings));
  const store = create({ Error, Promise, structuredClone }, { appId: 'retry' }, () => owner, 'store-key');
  const call = () => kind === 'sqlite' ? store.capability.sqlite.open('app:/data/db') : store.capability.fs.readFile(kind === 'documents' ? 'doc:/file' : 'app:/data/file');
  for (let i = 0; i < 3; i++) {
    const result = Promise.allSettled([call()]);
    await store.deliver(owner);
    const [outcome] = await result;
    expect(outcome.status).toBe(i === 0 ? 'rejected' : 'fulfilled');
    if (i === 0) expect(outcome.reason.message).toBe('network');
    else if (kind === 'sqlite') expect(outcome.value.execute).toBeFunction();
    else expect(outcome.value).toBe('contents');
  }
  expect(attempts).toBe(2);
  expect(factories).toBe(1);
});

test('wasm GPU and geometry loads retry after a refusal', async () => {
  const glue = readFileSync(new URL('../web/glue.js', import.meta.url), 'utf8');
  const boot = readFileSync(new URL('../web/wasm/boot.js', import.meta.url), 'utf8');
  for (const kind of ['gpu', 'geometry']) {
    let attempts = 0, installed = 0;
    const host = {}, lines = [];
    const bindings = { loadOnce, host, root: {}, requestAnimationFrame: resolve => resolve(),
      console: { error: (...args) => lines.push(args) },
      globalThis: { exact: { pendingSurfaces: [{}] } },
      loadAfterPaint: () => ++attempts === 1 ? Promise.reject(new TypeError('network')) : Promise.resolve(() => { installed++; return {}; }),
    };
    const run = new Function(...Object.keys(bindings), glueDeclarations(kind === 'gpu' ? glue : boot, [kind === 'gpu' ? 'loadGpuIfNeeded' : 'loadGeometry']) + '; return ' + (kind === 'gpu' ? 'loadGpuIfNeeded' : 'loadGeometry'))(...Object.values(bindings));
    const key = kind === 'gpu' ? 'gpuLoading' : 'geometryLoading';
    run(); run(); await host[key];
    expect(lines.length).toBe(1);
    expect(host[key]).toBeNull();
    expect(attempts).toBe(1);
    run(); await host[key];
    run(); await host[key];
    expect(attempts).toBe(2);
    expect(installed).toBe(kind === 'gpu' ? 0 : 1);
  }
});

test('a wasm textarea leaves pending after a rejected editor load and installs on retry', async () => {
  const source = readFileSync(new URL('../web/wasm/views.js', import.meta.url), 'utf8');
  class TextArea { isConnected = true; dataset = { view: '1' }; getAttribute(name) { return name === 'markup' ? 'markdown' : null; } hasAttribute() { return false; } }
  const el = new TextArea(), host = {}, markupPending = new WeakSet(), pending = [], errors = [];
  let attempts = 0, installed = 0;
  const bindings = { loadOnce: (...args) => { const p = loadOnce(...args); pending.push(p); return p; },
    host, moduleReady: Promise.resolve(), HTMLTextAreaElement: TextArea, markupPending, composing: new WeakSet(),
    views: new Map([[1, el]]), retiredViews: new Set(), console: { error: (...args) => errors.push(args) },
    loadAfterPaint: () => ++attempts === 1 ? Promise.reject(new TypeError('network')) : Promise.resolve(() => installed++),
  };
  const run = new Function(...Object.keys(bindings), glueDeclarations(source, ['syncMarkup']) + '; return syncMarkup;')(...Object.values(bindings));
  run(el); run(el);
  await Promise.allSettled(pending.splice(0)); await Promise.resolve();
  expect(markupPending.has(el)).toBe(false);
  expect(errors.length).toBe(1);
  run(el);
  await Promise.allSettled(pending.splice(0)); await Promise.resolve();
  expect(installed).toBe(1);
  expect(markupPending.has(el)).toBe(false);
  expect(attempts).toBe(2);
});

test('a failed wasm module script leaves no element behind', async () => {
  const source = readFileSync(new URL('../web/glue.js', import.meta.url), 'utf8');
  const scripts = [], exact = {};
  let attempts = 0;
  const bindings = { URL, baseURL: 'https://exact.test/glue.js', globalThis: { exact },
    document: { createElement: () => ({ remove() { scripts.splice(scripts.indexOf(this), 1); } }),
      head: { append(script) { scripts.push(script); queueMicrotask(() => { if (++attempts === 1) script.onerror(); else { exact.picker = {}; script.onload(); } }); } } },
  };
  const load = new Function(...Object.keys(bindings), glueDeclarations(source, ['loadAfterPaint']).replaceAll('import.meta.url', 'baseURL') + '; return loadAfterPaint;')(...Object.values(bindings));
  expect((await Promise.allSettled([load('./picker-glue.js', 'picker')]))[0].status).toBe('rejected');
  expect(scripts.length).toBe(0);
  expect(await load('./picker-glue.js', 'picker')).toBe(exact.picker);
  expect(await load('./picker-glue.js', 'picker')).toBe(exact.picker);
  expect(attempts).toBe(2);
});

// One malformed line refuses the whole declaration here as in the runner
// (save_file.rs pins this JSON as `normalized_json` writes it).
test('a grant declaration with a malformed line admits no path', () => {
  const broken = createGrantSet(JSON.parse('{"version":1,"entries":[[1,"fs.read \\"app:/data/a b\\"",["fs-read","app:","data","a b"],null],[2,"fs.write relative",null,"`relative` must be an absolute, resolved path"]],"error":"the app\'s grants did not parse: line 2: `relative` must be an absolute, resolved path","seal":"3f46888c301bd92e"}'));
  expect(grantError(broken)).toBeTruthy();
  expect(coversPath(broken, 'fs.read', 'app:/data/a b/n.txt')).toBe(false);
  expect(coversPath(GRANTS, 'fs.read', 'app:/data/a b/n.txt')).toBe(true);
});

// The web glue (picker-glue.js) reads each app file into a File named by
// its last segment and typed by its extension, and shares where
// `navigator.canShare` takes them; a fake navigator stands in.
test('app files reach navigator.share as typed Files, guarded by canShare', async () => {
  globalThis.exact ??= {};
  await import(`../web/picker-glue.js?share=${Date.now()}`);
  const api = globalThis.exact.picker({ appId: 'test.share', dispatch() {}, pickedPath: async () => 'app:/tmp/picked/1.png', log() {} });
  await api.answer(5, [{ name: 'card.png', bytes: btoa('png') }]);
  const was = Object.getOwnPropertyDescriptor(globalThis, 'navigator'), shared = [];
  let takes = true, fails = null;
  Object.defineProperty(globalThis, 'navigator', { configurable: true, writable: true, value: {
    canShare: d => takes && d.files.every(f => f instanceof File),
    share: async d => { if (fails) throw Object.assign(new Error(fails), { name: fails }); shared.push(d); },
  } });
  try {
    const picked = [{ path: 'app:/tmp/picked/1.png' }];
    expect(await api.share({ title: 'T', text: null, url: null }, picked)).toBe('share: shared');
    expect(Object.keys(shared[0])).toEqual(['title', 'files']);
    expect(shared[0].files.map(f => [f.name, f.type, f.size])).toEqual([['1.png', 'image/png', 3]]);
    fails = 'AbortError';
    expect(await api.share({ title: 'T' }, picked)).toBe('share: dismissed');
    fails = 'NotAllowedError';
    expect(await api.share({ title: 'T' }, picked)).toBe('share: refused: NotAllowedError');
    takes = false;
    expect(await api.share({ title: 'T' }, picked)).toBe('share: refused: this browser cannot share these files');
    expect(await api.share({}, [{ path: 'app:/data/missing.txt' }])).toBe('share: refused: this page has no app files');
    expect(shared.length).toBe(1);
  } finally { Object.defineProperty(globalThis, 'navigator', was); }
});

// These tables run in separate Rust, bundled-JS and lazy-glue artifacts.
// Check every entry in both directions so adding a type to only one fails.
test('picker admission and delivery agree on every filename MIME type', async () => {
  const rust = readFileSync(new URL('../../runner/src/runner/picker.rs', import.meta.url), 'utf8');
  const body = rust.slice(rust.indexOf('const MIME_TYPES'), rust.indexOf('/// Whether `accept`'));
  const native = Object.fromEntries([...body.matchAll(/\("([a-z0-9]+)", "([^"]+)"\)/g)].map(([, ext, mime]) => [ext, mime]));
  const js = readFileSync(new URL('./files.js', import.meta.url), 'utf8');
  const table = new Function(glueDeclarations(js, ['MIME']) + '; return MIME;')();
  const source = readFileSync(new URL('../web/picker-glue.js', import.meta.url), 'utf8');
  const bodyJS = glueDeclarations(source, ['typeFor']);
  const typeFor = new Function(bodyJS + '; return typeFor;')();
  const delivered = Object.fromEntries([...bodyJS.matchAll(/\b(\w+): '/g)].map(([, ext]) => [ext, typeFor('file.' + ext)]));
  expect(Object.keys(native).length).toBeGreaterThan(30);
  expect(table).toEqual(native);
  expect(delivered).toEqual(native);
  globalThis.exact ??= {};
  await import('../web/picker-glue.js');
  const sent = [];
  const picker = exact.picker({appId: 'mime-test', pickedPath: name => 'app:/tmp/picked/' + name,
    dispatch: (id, kind, payload) => sent.push(payload), log() {}});
  for (const [ext, mime] of Object.entries(native)) {
    const name = 'file.' + ext.toUpperCase();
    await picker.answer(1, [{name, bytes: btoa('fixture')}]);
    expect(sent.at(-1).split('\t')[2].split(';', 1)[0], name).toBe(mime);
    expect(accepts([mime], name, table[ext])).toBe(true);
    expect(accepts([mime.split('/')[0] + '/*'], name, table[ext])).toBe(true);
  }
  for (const name of ['unknown.xyz', 'png', 'file.constructor', 'file.__proto__'])
    expect(typeFor(name)).toBe('application/octet-stream');
});

test('agent TIFF and Markdown picks deliver the type admitted by their names', async () => {
  globalThis.exact ??= {};
  await import('../web/picker-glue.js');
  const sent = [];
  const picker = exact.picker({appId: 'mime-test', pickedPath: name => 'app:/tmp/picked/' + name,
    dispatch: (id, kind, payload) => sent.push(payload), log() {}});
  await picker.answer(1, ['scan.tiff', 'scan.TIF', 'notes.markdown'].map(name => ({name, bytes: btoa('fixture')})));
  expect(sent[0].split('\n').map(line => line.split('\t')[2])).toEqual(['image/tiff', 'image/tiff', 'text/markdown']);
});
