import { expect, test } from 'bun:test';
import { fixture } from './surface.mjs';
import { createTransport } from '../gpu-jobs.js';
import { chromium, suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { decodePng } from '../../../scripts/png.mjs';

test('the GPU loads when its initializer returns nothing', async () => {
  let loads = 0;
  await fixture({ loadModule: true, gpuLoad: () => loads++ });
  expect(loads).toBe(1);
});

test('the GPU loads when setting up the worker binding throws', async () => {
  let loads = 0;
  const gpu = {
    default: () => ({ memory: new WebAssembly.Memory({ initial: 1 }) }),
    exact_game_worker_module() {},
  };
  // The fixture's global has no pagehide listener API.
  const f = await fixture({ loadModule: true, gpu, gpuLoad: () => loads++ });
  expect(loads).toBe(1);
  expect(f.diagnostics.some(message => message.includes('using inline jobs'))).toBe(true);
});

test('a rejected worker script load removes the binding and warns', async () => {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const bindingRemoved = Promise.withResolvers();
  let loads = 0;
  const f = await fixture({
    loadModule: true,
    gpu: { default: () => ({ memory }), exact_game_worker_module() {} },
    globals: { addEventListener() {}, removeEventListener: bindingRemoved.resolve },
    loadJobs: async () => { throw new Error('worker script unavailable'); },
    gpuLoad: () => loads++,
  });
  f.exact.gpu.jobs.get(memory).start(['mesh']);
  await bindingRemoved.promise;
  expect(f.exact.gpu.jobs.has(memory)).toBe(false);
  expect(loads).toBe(1);
  expect(f.diagnostics.filter(message => message.includes('worker script unavailable'))).toHaveLength(1);
});

test('a module that declines the worker transport removes its binding and pagehide listener', async () => {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const listeners = new Map();
  let imports = 0;
  const f = await fixture({
    loadModule: true,
    gpu: { default: () => ({ memory }), exact_game_worker_module() {} },
    globals: {
      addEventListener(name, listener) { listeners.set(name, listener); },
      removeEventListener(name, listener) {
        expect(listeners.get(name)).toBe(listener);
        listeners.delete(name);
      },
    },
    loadJobs: () => { imports++; },
  });
  expect(listeners.has('pagehide')).toBe(true);
  f.exact.gpu.jobs.get(memory).discard();
  expect(f.exact.gpu.jobs.has(memory)).toBe(false);
  expect(listeners.has('pagehide')).toBe(false);
  expect(imports).toBe(0);
});

test.each([
  ['readyMs includes script import time', 4100],
  ['worker readiness shares the import deadline', 5100],
  ['a late script import creates no workers', null],
])('%s', async (_name, readyAt) => {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const script = Promise.withResolvers(), timers = new Map(), workers = [], warnings = [];
  let now = 0, timer = 0;
  const clock = {
    now: () => now,
    setTimeout(fn, ms) { timers.set(++timer, { at: now + ms, fn }); return timer; },
    clearTimeout(id) { timers.delete(id); },
  };
  const advance = at => {
    now = at;
    for (const [id, timer] of timers) {
      if (timer.at > now) continue;
      timers.delete(id);
      timer.fn();
    }
  };
  class Worker {
    constructor() { workers.push(this); }
    postMessage() {}
    terminate() {}
  }
  const f = await fixture({
    loadModule: true,
    gpu: { default: () => ({ memory }), exact_game_worker_module() {} },
    globals: { addEventListener() {}, removeEventListener() {} },
    performance: clock,
    setTimeout: clock.setTimeout,
    clearTimeout: clock.clearTimeout,
    loadJobs: () => script.promise,
  });
  const binding = () => f.exact.gpu.jobs.get(memory);
  advance(100);
  binding().start(['mesh']);
  expect(binding().events(1)).toEqual([0, []]);
  advance(readyAt === null ? 5200 : 3100);
  if (readyAt === null) {
    expect(binding()).toBeUndefined();
    expect(f.diagnostics.filter(message => message.includes('using inline jobs'))).toHaveLength(1);
  }
  script.resolve({ createTransport: options => createTransport({
    ...options, ...clock, Worker, cores: 1, warn: message => warnings.push(message),
  }) });
  await script.promise;

  if (readyAt === null) {
    expect(binding()?.events(1) ?? [-1, []]).toEqual([-1, []]);
    expect(workers).toHaveLength(0);
  } else {
    expect(workers).toHaveLength(1);
    advance(readyAt);
    workers[0].onmessage({ data: { ready: true, version: 1, names: ['mesh'] } });
    if (readyAt < 5100) {
      expect(binding().events(1)).toEqual([1, []]);
      expect(binding().report()[4]).toBe(4000);
    } else {
      expect(binding().events(1)).toEqual([-1, []]);
      expect(warnings).toHaveLength(1);
    }
  }
  if (readyAt === null) expect(f.diagnostics.filter(message => message.includes('using inline jobs'))).toHaveLength(1);
});

test('each GPU drawing element waits for a presented frame, including a second canvas of one surface', async () => {
  const f = await fixture();
  const hosts = [f.create(1), f.create(2)];
  for (const host of hosts) {
    expect(host.canvas.style.visibility).toBe('hidden');
    expect(host.style.visibility).toBeUndefined();
  }
  f.publish(1, {presented:false}); f.frame();
  expect(hosts[0].canvas.style.visibility).toBe('hidden');
  f.publish(2, {presented:true}); f.frame();
  expect(hosts[1].canvas.style.visibility).toBe('');
  expect(hosts[0].canvas.style.visibility).toBe('hidden');
  f.publish(1, {presented:true}); f.frame();
  expect(hosts[0].canvas.style.visibility).toBe('');
  // A later generation keeps the picture visible while its replacement prepares.
  f.publish(1, {presented:false}); f.frame();
  expect(hosts[0].canvas.style.visibility).toBe('');
  let empty = true;
  f.gpu.gpu_presentation_flags = id => id === 1 && empty ? 4 : 0;
  f.publish(1, {presented:false}); f.frame();
  expect(hosts[0].canvas.style.visibility).toBe('hidden');
  expect(hosts[1].canvas.style.visibility).toBe('');
  f.publish(1, {presented:true}); f.frame();
  expect(hosts[0].canvas.style.visibility).toBe('hidden');
  empty = false;
  f.publish(1, {presented:false}); f.frame();
  expect(hosts[0].canvas.style.visibility).toBe('hidden');
  f.publish(1, {presented:true}); f.frame();
  expect(hosts[0].canvas.style.visibility).toBe('');
});

test.each(['asset', 'render'])('a shader failure during %s reaches development even on a second canvas', async stage => {
  let read = 0, requested = false, refused = false;
  const detail = 'assets/avatar.wgsl: fragment pipeline validation failed';
  const f = await fixture({
    delivery: { fetch: async () => new Response('shader') },
    gpu: {
      gpu_error: () => { read++; return detail; },
      gpu_render: id => {
        if (stage !== 'render' || id !== 2 || refused) return 0;
        refused = true;
        return 2;
      },
      gpu_assets: id => {
        const load = stage === 'asset' && id === 2 && !requested;
        if (load) requested = true;
        return JSON.stringify({ requests: load ? ['avatar.wgsl'] : [], retired: [] });
      },
      gpu_asset: () => false,
    },
  });
  const messages = [];
  f.exact.devError = message => { if (message) messages.push(message); };
  f.create(1, 'shader');
  const second = f.create(2, 'shader');
  await f.exact.gpu.settled();
  f.frame();
  expect(read).toBe(1);
  expect(messages).toEqual([`surface shader: ${detail}`]);
  expect(f.diagnostics.filter(line => line.includes(detail))).toHaveLength(1);
  expect(second.canvas.style.visibility).toBe('hidden');
});

test.each(['render', 'flush'])('a malformed record published during %s leaves other canvases rendering', async stage => {
  const f = await fixture(), first = f.create(1, 'broken'), second = f.create(2, 'healthy');
  const pending = new Map(), order = [];
  const malformed = '{"value":NaN,"presented":false}';
  const publish = id => pending.set(id, id === 1 ? malformed : '{"presented":true}');
  f.gpu.gpu_dirty = () => true;
  f.gpu.gpu_render = id => {
    order.push(`render ${id}`);
    if (stage === 'render') publish(id);
    return 0;
  };
  f.gpu.gpu_flush = () => {
    order.push('flush');
    if (stage === 'flush') { publish(1); publish(2); }
    return true;
  };
  f.gpu.gpu_published = id => { const record = pending.get(id); pending.delete(id); return record; };

  expect(() => f.frame()).not.toThrow();
  expect(order).toEqual(['render 1', 'render 2', 'flush']);
  expect(f.records.slice(-2)).toEqual([`broken\0${malformed}`, 'healthy\0{"presented":true}']);
  expect(first.canvas.style.visibility).toBe('hidden');
  expect(second.canvas.style.visibility).toBe('');
});

test('replacement contexts wait for their own first complete frame', async () => {
  const f = await fixture(), host = f.create(1);
  f.publish(1, {presented:true}); f.frame();
  expect(host.canvas.style.visibility).toBe('');
  let replacement;
  f.nextGpu.gpu_create = (_name, el) => { replacement = el; return 2; };
  await f.exact.gpu.swap(1);
  expect(replacement.style.visibility).toBe('hidden');
  let record = '{"presented":true}';
  f.nextGpu.gpu_published = () => { const out = record; record = undefined; return out; };
  f.frame();
  expect(replacement.style.visibility).toBe('');
  const clone = f.Element.prototype.cloneNode;
  let recovered;
  f.Element.prototype.cloneNode = function () { recovered = clone.call(this); return recovered; };
  f.nextGpu.gpu_recover = async () => '{"status":"recovered"}';
  f.exact.gpu.deviceLost();
  await f.exact.gpu.settled();
  expect(recovered.style.visibility).toBe('hidden');
  record = '{"presented":true}'; f.frame();
  expect(recovered.style.visibility).toBe('');
  const active = recovered;
  f.nextGpu.gpu_recover = async () => '{"status":"no device"}';
  f.exact.gpu.deviceLost();
  await f.exact.gpu.settled();
  expect(active.style.visibility).toBe('hidden');
});

const browserTest = need('chrome', chromium({needs:['gpu']}).unavailable);

// A recording module over real WebGPU canvases: presented is the module's
// frame_ready fact after flush, independent of the surface's own record.
browserTest('an opaque preparation frame leaves the fallback and children visible until presentation', async () => {
  const moduleSource = `
    let device, next = 0, commands = [], rendered = [], records = new Map();
    const canvases = new Map();
    globalThis.completeFrame = id => { const c = canvases.get(id); c.ready = c.dirty = true; };
    globalThis.renderCounts = {};
    export default function() {}
    export async function gpu_load() {
      const adapter = await navigator.gpu.requestAdapter();
      device = await adapter.requestDevice();
    }
    export function gpu_reduced_motion() {}
    export function gpu_unload() { device?.destroy(); }
    export function gpu_child_view() {} export function gpu_seekable() {}
    export function gpu_shader_names() { return '[]'; } export function gpu_shaders_clear() {}
    export function gpu_create(name, canvas) {
      const context = canvas.getContext('webgpu');
      context.configure({device,format:navigator.gpu.getPreferredCanvasFormat(),alphaMode:'opaque'});
      const id = ++next;
      canvases.set(id,{context,ready:name === 'immediate'}); renderCounts[id] = 0;
      return id;
    }
    export function gpu_bind_at() { return true; } export function gpu_lifecycle() {}
    export function gpu_wants_input() { return false; } export function gpu_dirty(id) { return canvases.get(id).dirty; }
    export function gpu_period() {} export function gpu_agent() {}
    export function gpu_assets() { return '{"requests":[],"retired":[]}'; }
    export function gpu_render(id) {
      const {context,ready} = canvases.get(id), encoder = device.createCommandEncoder();
      const pass = encoder.beginRenderPass({colorAttachments:[{
        view:context.getCurrentTexture().createView(),loadOp:'clear',storeOp:'store',
        clearValue:ready ? [0,1,0,1] : [0,0,0,1],
      }]});
      pass.end(); commands.push(encoder.finish()); rendered.push(id); renderCounts[id]++;
      canvases.get(id).dirty = false;
      return 0;
    }
    export function gpu_flush() {
      if (commands.length) device.queue.submit(commands.splice(0));
      for (const id of rendered.splice(0)) records.set(id,JSON.stringify({presented:canvases.get(id).ready}));
      return true;
    }
    export function gpu_published(id) { const record = records.get(id); records.delete(id); return record; }
    export function gpu_messages() {}
  `;
  const html = `<!doctype html><style>
    body {margin:0;display:flex}
    .host {position:relative;isolation:isolate;width:64px;height:64px;background:#d02040}
    canvas {position:absolute;inset:0;width:100%;height:100%;z-index:-1}
    .child {width:8px;height:8px;background:#2040d0}
  </style><div class="host" id="delayed"><canvas data-surface></canvas><div class="child"></div></div>
  <div class="host" id="immediate"><canvas data-surface></canvas></div><script type="module">
    globalThis.records = [];
    globalThis.exact = {views:new Map([[1,document.getElementById('delayed')],[2,document.getElementById('immediate')]]),
      root:document.body,now:()=>0,devAssets:[],generation:0,writeIn:text=>text,
      wasm:{exact_surface_record(text){records.push(text);}},send(){}};
  </script>`;
  const server = Bun.serve({hostname:'127.0.0.1',port:0,fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response(html,{headers:{'content-type':'text/html'}});
    if (path === '/favicon.ico') return new Response(null,{status:204});
    if (path === '/gpu.js') return new Response(moduleSource,{headers:{'content-type':'text/javascript'}});
    return new Response(Bun.file(new URL(`..${path}`,import.meta.url)),{headers:{'content-type':'text/javascript'}});
  }});
  let browser;
  try {
    browser = await suiteBrowser('chromium',{needs:['gpu']});
    const page = await browser.newPage({viewport:{width:128,height:64}}), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    const pixels = async () => {
      const {width,data} = decodePng(await page.screenshot());
      return [[32,32],[96,32],[4,4]].map(([x,y]) => [...data.slice((y*width+x)*4,(y*width+x)*4+4)]);
    };
    await page.goto(server.url.href);
    await page.evaluate(async () => {
      await import('/gpu-glue.js');
      if (exact.gpu.recovery) throw new Error(exact.gpu.recovery.error);
      exact.gpu.surface(1, 'delayed', []);
      exact.gpu.surface(2, 'immediate', []);
      await exact.gpu.settled();
    });
    expect(await page.evaluate(() => renderCounts[1])).toBeGreaterThan(0);
    expect(await pixels()).toEqual([[208,32,64,255],[0,255,0,255],[32,64,208,255]]);
    await page.evaluate(async () => { completeFrame(1); exact.gpu.surface(1,'delayed',[]); await exact.gpu.settled(); });
    expect(await pixels()).toEqual([[0,255,0,255],[0,255,0,255],[32,64,208,255]]);
    expect(await page.evaluate(() => records.includes('delayed\0{"presented":true}'))).toBe(true);
    expect(errors).toEqual([]);
  } finally { await browser?.close(); server.stop(true); }
}, 60000);
