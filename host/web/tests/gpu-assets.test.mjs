import {assetDelivery} from '../gpu-assets.js';
import {test, expect} from 'bun:test';
import {existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, utimesSync, writeFileSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {resolve} from 'node:path';
const source = readFileSync(new URL('../gpu-glue.js', import.meta.url), 'utf8');
test('asset fetches have at most eight simultaneous requests', async () => {
  let active = 0, peak = 0;
  const release = [];
  const entry = {id: 1, view: 1};
  let names = Array.from({length: 32}, (_, i) => `${i}.model`);
  const gpu = {gpu_assets: () => JSON.stringify({requests:names.splice(0),retired:[]}), gpu_asset: () => true};
  const fetch = () => { peak = Math.max(peak, ++active); return new Promise(resolve => release.push(() => {
    active--; resolve(new Response(new Uint8Array(1)));
  })); };
  const api = assetDelivery({getModule:()=>gpu, live:()=>entry, fetch, baseURI:()=> 'http://fixture/'});
  api.assets(entry);
  const firstPeak = peak;
  while (api.assetFlights.size) { for (const finish of release.splice(0)) finish(); await new Promise(r=>setTimeout(r,0)); }
  expect(firstPeak).toBeLessThanOrEqual(8);
  expect(peak).toBeLessThanOrEqual(8);
});
test('a failed cosmetic never gets a first-frame stamp or repeated state serialization', () => {
  let reads = 0;
  const body = 'let recoveringDevice, recoveryTimer; const trapped = () => undefined; const retainTrap = error => { throw error; };\n' + source.slice(source.indexOf('function childFrames(entry)'), source.indexOf('// The live frame clock'));
  const gpu = {gpu_render:()=>0, gpu_agent:()=>{reads++; return JSON.stringify({world:{assets:[{name:'bad.model',state:'Failed'}]}})}};
  const render = new Function('gpu','hidden','exact','size','clockFor','messages','requestAnimationFrame',body+'return render;')(
    gpu, false, {}, ()=>({w:10,h:10,s:1}), x=>x, ()=>{}, ()=>{});
  const entry = {id:1,el:{}};
  render(entry,0); render(entry,1); render(entry,2);
  expect(entry.firstFrameSubmittedMs).toBeUndefined();
  expect(reads).toBe(1);
});

test('a final paused frame delivers submitted output once without another render', () => {
  const entry = {id:1,view:1,name:'world',el:{},host:{},wants:true};
  const surfaces = new Map([[entry.view,entry]]), delivered = [], records = [];
  let renders = 0, submits = 0, pending = false, output = [], record;
  const gpu = {
    gpu_render:()=>{ pending = true; return ++renders === 1 ? 1 : 0; },
    gpu_dirty:()=>false,
    gpu_flush:()=>{
      if (pending) {
        pending = false; submits++;
        if (submits === 2) { output.push('custom frame submitted'); record = '{"complete":true}'; }
      }
      return true;
    },
    // The first-frame diagnostic asks state, which settles the first render.
    gpu_agent:()=>{ gpu.gpu_flush(); return '{"world":{}}'; },
    gpu_published:()=>{ const next = record; record = undefined; return next; },
    gpu_messages:()=>output.length ? JSON.stringify(output.splice(0)) : undefined,
  };
  const body = `
    let recoveringDevice, recoveryTimer, terminalDevice = false, hidden = false;
    let raf = null, frameAt = null, frameRaw = 0, frameGeneration = 0, sentPeriod = 0, liveWindow = false;
    const trapped = () => false, retainTrap = error => { throw error; };
    const pace = Object.assign(now => now, {period_ms:0}), clockFor = now => now;
    const size = () => ({w:10,h:10,s:1}), supplyChildren = () => {}, placeChildren = () => {};
    const assets = () => {}, finishRestore = () => {}, reportRestore = () => {};
    const publishers = new Map([['world',entry]]), live = view => surfaces.get(view);
    const surfaceRecord = (name,json) => records.push([name,json]);
    const exact = {message:(host,text) => delivered.push([host,text])};
    const requestAnimationFrame = () => 1;
  ` + source.slice(source.indexOf('function flush('), source.indexOf('// The live frame clock'))
    + source.slice(source.indexOf('function frame(now)'), source.indexOf('// An editable element'))
    + source.slice(source.indexOf('function performanceRecord(entry)'), source.indexOf('function schedulePerformance()'))
    + source.slice(source.indexOf('function messages(entry'), source.indexOf('function listen(entry)'));
  const api = new Function('globalThis','gpu','surfaces','entry','delivered','records',body + 'return {frame,flush};')(
    {exact:{}},gpu,surfaces,entry,delivered,records);
  api.frame(0);
  expect(delivered).toEqual([]);
  api.frame(16);
  expect(entry.wants).toBe(false);
  expect(delivered).toEqual([[entry.host,'custom frame submitted']]);
  expect(records).toEqual([['world','{"complete":true}']]);
  api.flush(); api.frame(32);
  expect([renders,submits]).toEqual([2,2]);
  expect(delivered).toHaveLength(1);
  expect(records).toHaveLength(1);
});

test('browser frame scheduling delivers final submitted output through the complete GPU glue', async () => {
  const {suiteBrowser} = await import('../../../scripts/agent-launch.mjs');
  const moduleSource = `
    let armed = false, pending = false, record, output = [];
    const counts = globalThis.counts = {renders:0, submits:0, assetPolls:0};
    globalThis.arm = () => { armed = true; counts.assetPolls = 0; };
    export default function() {}
    export function gpu_load() {} export function gpu_child_view() {}
    export function gpu_shader_names() { return '[]'; } export function gpu_shaders_clear() {}
    export function gpu_create() { return 1; } export function gpu_bind_at() { return true; }
    export function gpu_lifecycle() {} export function gpu_wants_input() { return false; }
    export function gpu_period() {} export function gpu_dirty() { return armed && counts.renders === 0; }
    export function gpu_agent() { return '{"world":{}}'; }
    export function gpu_assets() { if (armed) counts.assetPolls++; return '{"requests":[],"retired":[]}'; }
    export function gpu_render() {
      if (!armed) return 0;
      pending = true; return ++counts.renders === 1 ? 1 : 0;
    }
    export function gpu_flush() {
      if (pending) {
        pending = false;
        if (++counts.submits === 2) { output.push('final submission'); record = '{"complete":true}'; }
      }
      return true;
    }
    export function gpu_published() { const next = record; record = undefined; return next; }
    export function gpu_messages() { return output.length ? JSON.stringify(output.splice(0)) : undefined; }
  `;
  const html = `<div id="host" style="width:200px;height:120px"><canvas data-surface style="display:block;width:100%;height:100%"></canvas></div><script type="module">
    Object.defineProperty(Navigator.prototype,'gpu',{get:()=>({requestAdapter:async()=>({info:{}})}),configurable:true});
    const host = document.getElementById('host');
    globalThis.records = []; globalThis.messages = [];
    globalThis.exact = {views:new Map([[1,host]]),root:document.body,devAssets:[],generation:0,
      writeIn:text=>text, wasm:{exact_surface_record(text){ records.push(text); return 0; }}, send(){},
      message(target,text){ messages.push([target.id,text]); }};
    await import('/gpu-glue.js'); exact.gpu.surface(1,'world',[]);
    await exact.gpu.settled();
    // Finish the initial ResizeObserver delivery before starting the two-frame world.
    await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
    arm(); exact.gpu.schedule();
  </script>`;
  const server = Bun.serve({hostname:'127.0.0.1',port:0,fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response(html,{headers:{'content-type':'text/html'}});
    if (path === '/favicon.ico') return new Response(null,{status:204});
    if (path === '/gpu.js') return new Response(moduleSource,{headers:{'content-type':'text/javascript'}});
    const file = path === '/gpu-glue.js' && process.env.E2B_GPU_SOURCE ? process.env.E2B_GPU_SOURCE : new URL(`..${path}`,import.meta.url);
    return new Response(Bun.file(file),{headers:{'content-type':'text/javascript'}});
  }});
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage(), errors = [];
    page.on('pageerror',error=>errors.push(error.message));
    await page.goto(server.url.href);
    try { await page.waitForFunction(()=>globalThis.counts?.submits === 2); }
    catch (error) {
      error.message += `; fixture ${JSON.stringify(await page.evaluate(()=>({counts:globalThis.counts,messages:globalThis.messages,records:globalThis.records})))}; page errors ${JSON.stringify(errors)}`;
      throw error;
    }
    expect(await page.evaluate(()=>({messages,records,counts}))).toEqual({
      messages:[['host','final submission']], records:['world\0{"complete":true}'],
      counts:{renders:2,submits:2,assetPolls:2},
    });
    await page.evaluate(async () => {
      exact.gpu.schedule();
      await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
    });
    expect(await page.evaluate(()=>({messages,records,counts}))).toEqual({
      messages:[['host','final submission']], records:['world\0{"complete":true}'],
      counts:{renders:2,submits:2,assetPolls:2},
    });
    expect(errors).toEqual([]);
  } finally { await browser?.close(); server.stop(true); }
}, 60000);

function harness({names, fetch, readAppFile, exact = {}}) {
  const entry = {id: 1, view: 1}, delivered = [], failed = [];
  let retired = [];
  const gpu = {gpu_assets: () => JSON.stringify({requests:names.splice(0),retired:retired.splice(0)}),
    gpu_asset: (_, name, bytes) => { delivered.push([name,bytes]); return true; },
    gpu_asset_failed: (_, name, reason) => { failed.push([name,reason]); return true; }};
  const api = assetDelivery({getModule:()=>gpu, live:()=>entry, fetch, readAppFile, devAssets:()=>exact.devAssets, baseURI:()=> 'http://fixture/'});
  return {...api, entry, delivered, failed, retire: n => retired.push(n)};
}
test('GPU app files resolve before bundled assets, including in a dev generation', async () => {
  for (const devAssets of [undefined, new Map([['assets/part.glb', {bytes:new Uint8Array([2])}]])]) {
    const names = ['app:/cache/part.glb', 'app:/data/part.glb', 'app:/tmp/part.glb', 'app:/cache/missing.glb', 'part.glb'];
    const resolved = [], fetched = [];
    const api = harness({names:[...names], exact:{devAssets},
      readAppFile:async name => { resolved.push(name); return name.includes('missing') ? null : new Blob([new Uint8Array([2])]); },
      fetch:async url => { fetched.push(String(url)); return new Response(new Uint8Array([2])); }});
    await api.settled(()=>[api.entry]);
    expect(resolved).toEqual(names.slice(0,4));
    expect(fetched).toEqual(devAssets ? [] : ['http://fixture/assets/part.glb']);
    expect(api.failed).toEqual([]);
    expect(api.delivered.map(([name,bytes])=>[name,bytes && [...bytes]]).sort()).toEqual(names.map(name=>[name,name.includes('missing') ? null : [2]]).sort());
  }
});
test('GPU asset names are validated before resolving app files or bundled assets', async () => {
  const names = ['app:/cache/../part.glb', 'app:/cache/./part.glb', 'app:/cache/café.glb', `app:/cache/${'a'.repeat(118)}`, '../part.glb'];
  const api = harness({names:[...names], readAppFile:()=>{ throw new Error('must not resolve'); }, fetch:()=>{ throw new Error('must not fetch'); }});
  await api.settled(()=>[api.entry]);
  expect(api.delivered).toEqual([]);
  expect(api.failed).toEqual(names.map(name=>[name,'invalid asset name']));
});
test('GPU app files obey the pre-copy byte limit', async () => {
  let copied = false;
  const api = harness({names:['app:/cache/large.glb'],
    readAppFile:async () => ({size:64*1024*1024+1,arrayBuffer(){ copied = true; }}),
    fetch:()=>{ throw new Error('must not fetch'); }});
  await api.settled(()=>[api.entry]);
  expect(api.delivered).toEqual([]);
  expect(api.failed).toEqual([['app:/cache/large.glb','exceeds 64 MiB']]);
  expect(copied).toBe(false);
});
test.each([['js', 'connect-src'], ['wasm', 'connect-src'], ['js', 'default-src'], ['wasm', 'default-src']])('%s GPU app files use the media resolver under %s self', async (target, directive) => {
  const {suiteBrowser} = await import('../../../scripts/agent-launch.mjs');
  const fixture = `
    import {assetDelivery} from '/gpu-assets.js';
    const entry = {id:1,view:1}, delivered = [], failed = [];
    const names = ['app:/cache/part.glb','app:/cache/missing.glb','app:/other/part.glb'];
    const gpu = {gpu_assets:()=>JSON.stringify({requests:names.splice(0),retired:[]}),
      gpu_asset:(_,name,bytes)=>{ delivered.push([name,bytes && [...bytes]]); return true; },
      gpu_asset_failed:(_,name,reason)=>{ failed.push([name,reason]); return true; }};
    const delivery = assetDelivery({getModule:()=>gpu,live:()=>entry,baseURI:()=>document.baseURI,devAssets:()=>new Map()});
    exact.gpu = {async settled() {
      await delivery.settled(()=>[entry]);
      globalThis.assetResult = {delivered:delivered.sort(),failed};
    }};
  `;
  const server = Bun.serve({hostname:'127.0.0.1',port:0,fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response('<!doctype html><div id="exact-root"></div>',{headers:{
      'content-type':'text/html',
      'content-security-policy':`default-src 'self'; img-src 'self' blob:;${directive === 'connect-src' ? " connect-src 'self';" : ''}`,
    }});
    if (path === '/host/web-js/names.js') return new Response('export default [[],[],[]]; export const types = [[],[],[]], sourceTypes = {};',{headers:{'content-type':'text/javascript'}});
    if (path === '/gpu-glue.js') return new Response(fixture,{headers:{'content-type':'text/javascript'}});
    let file = new URL(path.startsWith('/host/') ? `../../..${path}` : `..${path}`,import.meta.url);
    if (!existsSync(file)) file = new URL(file.href.replace('/host/web-js/', '/host/web/'));
    return new Response(Bun.file(file),{headers:{'content-type':'text/javascript'}});
  }});
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.evaluate(async target => {
      const appId = 'gpu-asset-test';
      globalThis.exact = {compat:{inputs:target === 'wasm' ? {app:appId} : {gpuModules:{}}}};
      const {createFileStore} = await import('/storage-fs.js');
      const store = createFileStore(appId);
      try {
        await store.writeFile('app:/cache/part.glb',new Uint8Array([4,2]));
        await store.writeFile('app:/cache/preview.svg',new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg" width="2" height="3"/>'));
      }
      finally { store.close(); }
      if (target === 'js') {
        const rt = await import('/host/web-js/rt.js');
        rt.data.appId = appId;
        rt.mount(root => {
          const canvas = document.createElement('div');
          root.append(canvas);
          rt.gs(canvas,'model',()=>[]);
        });
      } else {
        await import('/gpu-glue.js');
        await exact.gpu.settled();
      }
    }, target);
    await page.waitForFunction(()=>globalThis.assetResult);
    expect(await page.evaluate(()=>assetResult)).toEqual({delivered:[['app:/cache/missing.glb',null],['app:/cache/part.glb',[4,2]],['app:/other/part.glb',null]],failed:[]});
    const image = await page.evaluate(async () => {
      const image = new Image();
      image.src = await exact.appURL('app:/cache/preview.svg','gpu-asset-test');
      await image.decode();
      return [image.naturalWidth,image.naturalHeight];
    });
    expect(image).toEqual([2,3]);
  } finally { await browser?.close(); server.stop(true); }
}, 60000);
test.each([false, true])('a production checkpoint reads GPU app files before its TypeScript module loads (variant: %s)', async variant => {
  const {suiteBrowser} = await import('../../../scripts/agent-launch.mjs');
  const {patchLines} = await import('../../../scripts/app.mjs');
  const {webCompiler} = await import('../../web-js/module.mjs');
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../../..'), dir = mkdtempSync(resolve(root, 'target/gpu-app-startup-'));
  const out = resolve(dir, 'dist'), baseId = 'test.gpu-app-startup', appId = baseId + (variant ? '.preview' : '');
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({name:'Startup',app:{id:baseId,name:'Startup'},rust:false,
      variants:{preview:{idSuffix:'.preview',name:'Startup Preview'}}}));
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=build activate=idle

component App
  state count = 0
  resource said = greeting(count) as shape string
  action bump
    count = count + 1
  view
    column
      canvas surface=model() width=100 height=100
      text said testId="said"
      button "More" press=bump testId="bump"
`);
    writeFileSync(resolve(dir, 'app.ts'), "export const grants = '';\nexport const answer = (_: string, args: unknown[]): any => 'said ' + String(args[0]);\n");
    // A cached module registers the surface without compiling a renderer.
    const cache = resolve(dir, 'target/web-js-gpu/startup'), deps = resolve(dir, 'target/wasm32-unknown-unknown/web');
    for (const path of [resolve(dir, 'gpu/src'), cache, deps]) mkdirSync(path,{recursive:true});
    writeFileSync(resolve(dir, 'gpu/src/lib.rs'), '');
    writeFileSync(resolve(dir, 'gpu/Cargo.toml'), '[package]\nname="startup-gpu"\nversion="0.1.0"\nedition="2021"\n');
    writeFileSync(resolve(dir, 'Cargo.toml'), `[workspace]\nmembers=["gpu"]\nresolver="2"\n[patch.crates-io]\n${patchLines(dir).join('\n')}\n`);
    const locked = spawnSync('cargo',['generate-lockfile','--offline'],{cwd:dir,encoding:'utf8'});
    expect(locked.status,locked.stderr).toBe(0);
    writeFileSync(resolve(cache, 'gpu.js'), 'export function initSync() {}\nexport function gpu_surfaces() { return \'["model"]\'; }\n');
    writeFileSync(resolve(cache, 'gpu_bg.wasm'), '');
    writeFileSync(resolve(cache, '.modules.json'), JSON.stringify([['gpu','startup-gpu']]));
    writeFileSync(resolve(deps, 'startup_gpu.d'), `startup_gpu: ${resolve(dir, 'app.json')}\n`);
    utimesSync(resolve(dir, 'app.json'),0,0);
    const built = spawnSync(process.execPath,['host/web-js/build.mjs','startup','--out',out,'--production','--render','js'],{
      cwd:root,env:{...process.env,EXACT_APP_DIR:dir,EXACT_VARIANT:variant ? 'preview' : '',EXACT_VARIANT_APP:''},encoding:'utf8',
    });
    expect(built.status,built.stdout+built.stderr).toBe(0);
    expect(Object.keys(JSON.parse(readFileSync(resolve(out, '.gen/modules.json'),'utf8')))).not.toContain('ts-data.js');
    const chunk = readdirSync(out).find(name=>/^ts-data-.*\.js$/.test(name));
    expect(chunk).toBeDefined();
    const fixture = `
      import {assetDelivery} from '/test/gpu-assets.js';
      const entry = {id:1,view:1}, names = ['app:/cache/part.glb'];
      const gpu = {gpu_assets:()=>JSON.stringify({requests:names.splice(0),retired:[]}),
        gpu_asset:(_,name,bytes)=>{ (globalThis.assetResults ??= []).push([name,bytes && [...bytes]]); return true; }};
      const delivery = assetDelivery({getModule:()=>gpu,live:()=>entry,baseURI:()=>document.baseURI});
      exact.gpu = {destroy(){},surface(){},settled:()=>delivery.settled(()=>[entry])};
      globalThis.reread = () => { names.push('app:/cache/part.glb'); return exact.gpu.settled(); };
    `;
    server = Bun.serve({hostname:'127.0.0.1',port:0,fetch(request) {
      const path = new URL(request.url).pathname;
      if (path === '/seed') return new Response('<!doctype html>',{headers:{'content-type':'text/html'}});
      if (path === '/gpu-glue.js') return new Response(fixture,{headers:{'content-type':'text/javascript'}});
      const file = path.startsWith('/test/') ? resolve(root,'host/web',path.slice(6)) : resolve(out,path === '/' ? 'index.html' : path.slice(1));
      return new Response(Bun.file(file));
    }});
    browser = await suiteBrowser();
    const page = await browser.newPage(), errors = [];
    page.on('pageerror',error=>errors.push(error.message));
    await page.goto(new URL('/seed',server.url).href);
    await page.evaluate(async appId => {
      const {createFileStore} = await import('/test/storage-fs.js'), store = createFileStore(appId);
      try {
        await store.writeFile('app:/cache/part.glb',new Uint8Array([4,2]));
        await store.writeFile('app:/cache/preview.svg',new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg" width="2" height="3"/>'));
      } finally { store.close(); }
    },appId);
    await page.goto(server.url.href);
    await page.waitForFunction(()=>globalThis.assetResults?.length === 1);
    expect(await page.evaluate(()=>({results:assetResults,loaded:typeof exact.moduleReaders === 'function'})))
      .toEqual({results:[['app:/cache/part.glb',[4,2]]],loaded:false});
    await page.getByTestId('bump').click();
    await page.waitForFunction(()=>document.querySelector('[data-testid=said]').textContent === 'said 1');
    expect(await page.evaluate(()=>typeof exact.moduleReaders === 'function')).toBe(true);
    await page.evaluate(()=>reread());
    expect(await page.evaluate(()=>assetResults)).toEqual([['app:/cache/part.glb',[4,2]],['app:/cache/part.glb',[4,2]]]);
    expect(await page.evaluate(async appId => {
      const image = new Image();
      image.src = await exact.appURL('app:/cache/preview.svg',appId);
      await image.decode();
      return [image.naturalWidth,image.naturalHeight];
    },appId)).toEqual([2,3]);
    expect(errors).toEqual([]);
  } finally { await browser?.close(); server?.stop(true); rmSync(dir,{recursive:true,force:true}); }
}, 60000);
test('stream limit aborts before joining an oversized chunked response', async () => {
  let cancelled = false, read = 0;
  const api = harness({names:['large.tex'], fetch: async () => new Response(new ReadableStream({
    pull(c) { read++; if (read > 4) c.close(); else c.enqueue(new Uint8Array(32 * 1024 * 1024)); },
    cancel() { cancelled = true; },
  }))});
  api.assets(api.entry);
  await Promise.all([...api.assetFlights].map(f=>f.promise));
  expect(api.delivered).toEqual([]);
  expect(api.failed[0][1]).toContain('64 MiB');
  expect(cancelled).toBe(true);
  expect(read).toBeLessThanOrEqual(4);
});
test('dev assets obey the same pre-copy byte limit', async () => {
  const api = harness({names:['large.tex'], exact:{devAssets:new Map([['assets/large.tex',{bytes:{length:64*1024*1024+1}}]])}});
  api.assets(api.entry);
  await Promise.all([...api.assetFlights].map(f=>f.promise));
  expect(api.delivered).toEqual([]);
  expect(api.failed[0][1]).toContain('64 MiB');
});
test('retirement cancels queued and active names beyond 256 assets', async () => {
  const names = Array.from({length:300},(_,i)=>`${i}.model`);
  const api = harness({names, fetch: (_, {signal}) => new Promise((_, reject) => signal.addEventListener('abort',()=>reject(new Error('aborted'))))});
  api.assets(api.entry);
  const count = api.assetFlights.size;
  api.retire('0.model'); api.retire('20.model'); api.assets(api.entry);
  await new Promise(r=>setTimeout(r,0));
  const remaining = [...api.assetFlights].map(f=>f.name);
  api.cancelAssets(api.entry);
  await Promise.all([...api.assetFlights].map(f=>f.promise));
  expect(count).toBe(300);
  expect(api.failed).toEqual([]);
  expect(remaining).not.toContain('0.model');
  expect(remaining).not.toContain('20.model');
});

test('deferred open retains its carrier and a late refusal journals once', async () => {
  for (const refused of [true, false]) {
    const entry = {id:1,view:1,name:'world'}, exact = {worldCarry:new Uint8Array([1])}, journal = [];
    let names = ['crate.model'], restored = false;
    const gpu = {gpu_carry:()=>undefined, gpu_agent:()=>JSON.stringify({world:{restored}}), gpu_restore:()=>true,
      gpu_assets:()=>JSON.stringify({requests:names.splice(0),retired:[]}), gpu_error:()=>"invalid save",
      gpu_asset:()=>{ restored = !refused; return !refused; }};
    const restoreBody = source.slice(source.indexOf('function reportRestore('), source.indexOf('async function settled('))
      + source.slice(source.indexOf('function restorePending('), source.indexOf('function ensure('));
    const api = new Function('assetDelivery','gpu','exact','live','fetch','messages','schedule','restoreJournal','worldSize',
      restoreBody + `
const delivery = assetDelivery({getModule:()=>gpu, live, fetch, baseURI:()=> 'http://fixture/', delivered:finishRestore});
      return {...delivery, restorePending};`)(
      assetDelivery, gpu, exact, ()=>entry, async()=>new Response(new Uint8Array([1])), ()=>{}, ()=>{}, journal, x=>x);
    const bytes = exact.worldCarry;
    api.restorePending(entry);
    expect(exact.worldCarry).toBe(bytes);
    expect(entry.restoredCarry).toBeUndefined();
    api.assets(entry);
    await Promise.all([...api.assetFlights].map(f=>f.promise));
    api.assets(entry);
    if (refused) {
      expect(entry.restoreError).toContain('invalid save');
      expect(journal.length).toBe(1);
      expect(exact.worldCarry).toBe(bytes);
    } else {
      expect(entry.restoreError).toBeUndefined();
      expect(exact.worldCarry).toBeUndefined();
      expect(entry.restoredCarry).toBe(true);
    }
  }
});

test('retirement makes room for a full replacement batch before aborts settle', async () => {
  const names = Array.from({length:256},(_,i)=>`${i}.tex`), batch = [...names];
  const api = harness({names, fetch:(_, {signal})=>new Promise((_,reject)=>signal.addEventListener('abort',()=>reject(new Error('aborted'))))});
  api.assets(api.entry);
  for (const name of batch) api.retire(name);
  names.push(...batch); api.assets(api.entry);
  const failures = [...api.failed];
  api.cancelAssets(api.entry);
  await new Promise(r=>setTimeout(r,0));
  expect(failures).toEqual([]);
});

test('surface asset delivery keeps the live instance and commits only accepted bytes', async () => {
  const start = source.indexOf('  async reloadAssets('), end = source.indexOf('\n  // Dev only;', start);
  const entry = {id:7,view:3}, old = {bytes:new Uint8Array([1])}, next = {bytes:new Uint8Array([2])};
  const exact = {mutate:fn=>fn(),devAssets:new Map([['assets/tuning.json',old]])}, calls = [];
  let accepted = true;
  const gpu = {gpu_asset:(id,name,bytes)=>{calls.push({id,name,bytes});return accepted;},gpu_error:()=> 'invalid tuning field'};
  const api = new Function('ready','exact','surfaces','gpu','cancelAssets','messages','schedule',`return {${source.slice(start,end)}}`)(
    Promise.resolve(),exact,new Map([[3,entry]]),gpu,()=>{},()=>{},()=>{});
  const cards = new Map([['assets/tuning.json',next]]);
  expect(await api.reloadAssets(cards,['assets/tuning.json'],()=>false)).toBe(false);
  expect(calls).toHaveLength(0);
  expect(await api.reloadAssets(cards,['assets/tuning.json'])).toBe(true);
  expect(calls).toEqual([{id:7,name:'tuning.json',bytes:next.bytes}]);
  expect(entry.id).toBe(7);
  expect(exact.devAssets.get('assets/tuning.json')).toBe(next);
  accepted = false;
  await expect(api.reloadAssets(new Map([['assets/tuning.json',old]]),['assets/tuning.json'])).rejects.toThrow('invalid tuning field');
  expect(exact.devAssets.get('assets/tuning.json')).toBe(next);
});

// A dev card that reads its bytes when asked (dev.js `later`).
const lazy = bytes => { const held = {bytes:null, reading:null}; let reads = 0;
  return {held, get reads() { return reads; }, get bytes() { return held.bytes; },
    read: async () => { reads++; return held.bytes = bytes; }}; };
test('a dev card that reads when asked is read for the request alone', async () => {
  const asked = lazy(new Uint8Array([7])), unused = lazy(new Uint8Array([8]));
  const api = harness({names:['asked.tex'], exact:{devAssets:new Map([['assets/asked.tex',asked],['assets/unused.tex',unused]])}});
  api.assets(api.entry);
  await Promise.all([...api.assetFlights].map(f=>f.promise));
  expect(api.delivered).toEqual([['asked.tex',new Uint8Array([7])]]);
  expect([asked.reads, unused.reads]).toEqual([1, 0]);
});
test('an asset swap reads only the files surfaces asked for', async () => {
  const start = source.indexOf('  async reloadAssets('), end = source.indexOf('\n  // Dev only;', start);
  const entry = {id:7,view:3}, read = lazy(new Uint8Array([1])), unread = lazy(new Uint8Array([2]));
  await read.read();
  const exact = {mutate:fn=>fn(),devAssets:new Map([['assets/read.tex',read],['assets/unread.tex',unread]])}, calls = [];
  const gpu = {gpu_asset:(id,name,bytes)=>{calls.push([name,bytes]);return true;}};
  const api = new Function('ready','exact','surfaces','gpu','cancelAssets','messages','schedule',`return {${source.slice(start,end)}}`)(
    Promise.resolve(),exact,new Map([[3,entry]]),gpu,()=>{},()=>{},()=>{});
  const nextRead = lazy(new Uint8Array([3])), nextUnread = lazy(new Uint8Array([4]));
  const cards = new Map([['assets/read.tex',nextRead],['assets/unread.tex',nextUnread]]);
  expect(await api.reloadAssets(cards,['assets/read.tex','assets/unread.tex'])).toBe(true);
  expect(calls).toEqual([['read.tex',new Uint8Array([3])]]);
  expect([nextRead.reads, nextUnread.reads]).toEqual([1, 0]);
  expect(exact.devAssets.get('assets/unread.tex')).toBe(nextUnread);
});
