// App-owned virtual-list restoration through the browser runtime and real DOM geometry.
import { beforeAll, afterAll, test as bunTest, expect } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';
import { cdpBrowser, chromium, DESKTOP_INPUT } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';

const { unavailable } = chromium();
const test = need('chrome', unavailable, { test: bunTest });
let server, cdp, evaluate, call;
// Register the shared browser's shutdown hook before beforeAll runs.
const suite = unavailable ? null : await cdpBrowser({ args: [DESKTOP_INPUT] });
beforeAll(async () => {
  if (unavailable) return;
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === '/names.js') return new Response('export const sourceTypes = {};', {headers: {'content-type':'text/javascript'}});
    // A module that re-exports the host's own reaches it through `../web/`.
    const file = path.startsWith('/web/') ? `..${path}` : existsSync(new URL(`.${path}`, import.meta.url)) ? `.${path}` : `../web${path}`;
    return new Response(path.endsWith('.js') ? readFileSync(new URL(file, import.meta.url)) : '<!doctype html><body><main id="exact-root"></main>', {
      headers: { 'content-type': path.endsWith('.js') ? 'text/javascript' : 'text/html' },
    });
  }});
  ({ cdp } = suite);
  const { sessionId } = await suite.page();
  call = (method, params) => cdp.send(method, params, sessionId);
  evaluate = async expression => {
    const result = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
    return result.result.value;
  };
  await call('Page.navigate', { url: `http://127.0.0.1:${server.port}` });
  await evaluate(`Promise.all([import('/rt.js'), import('/list.js')]).then(([rt, list]) => { globalThis.rt = rt; globalThis.list = list; rt.clock.agent = true; })`);
}, 60_000); // hang bounds: Chrome's launch and exit are awaited as events, and take longer than 5 s under load
afterAll(async () => {
  await suite?.close();
  server?.stop(true);
}, 60_000);

test('an animated final row cannot remove the fixed trail across its window boundary', async () => {
  const result = await evaluate(`(() => {
    const port = document.createElement('div'); document.querySelector('main').append(port);
    const own = rt.scope(() => list.vl(port, () => [], i => i(), () => {}, {
      est:40, pw:[200], ph:[200], fixed:[() => {}, el => { const footer=document.createElement('div'); footer.textContent='Footer'; el.append(footer); }]
    }));
    const c=port.$list, leaving=document.createElement('div'); leaving.setAttribute('data-exiting','');
    port.insertBefore(leaving,c.fixed.b); c.children=[leaving]; c.emit();
    const kept=port.lastChild.textContent; rt.end(own); port.remove(); return kept;
  })()`);
  expect(result).toBe('Footer');
});

test('nested restoration keeps data below the lead and positions wholly inside fixed content', async () => {
  const result = await evaluate(`(() => {
    const port=document.createElement('div');document.querySelector('main').append(port);
    const own=rt.scope(()=>list.vl(port,()=>[1,2,3],i=>i(),()=>{}, {est:100,pw:[300],ph:[200],fixed:[()=>{},()=>{}]}));
    const c=port.$list; c.index.fixed=[300,800];
    const offsets=[];
    for(const at of [150,850]) {
      c.geometry={offset:at,port_main:200};const kept=c.position();
      c.geometry=null;c.index.fixed=[0,0];c.restorePosition(kept);
      offsets.push(c.correction.offset);c.index.fixed=[300,800];
    }
    rt.end(own);port.remove();return offsets;
  })()`);
  expect(result).toEqual([150, 850]);
});

test('initial follow measures fixed geometry before choosing an end anchor', async () => {
  const result = await evaluate(`(() => {
    const offsets=[];
    for(const items of [[1],[]]) for(const start of ['start','end']) {
      const port=document.createElement('div');document.querySelector('main').append(port);
      const own=rt.scope(()=>list.vl(port,()=>items,i=>i(),()=>{}, {est:100,pw:[300],ph:[200],atEnd:start==='end',fixed:[()=>{},()=>{}]}));
      const c=port.$list; c.followEnd=true; const byView=new Map(c.mounted.map((m,i)=>[m.view,i]));
      const measurements=c.fixedItems().map((item,i)=>({...item,size:[600,800][i]}));
      c.feedback({offset:0,port_main:200,port_cross:300,cross:300,padding:[0,0],measurements},byView,{});
      offsets.push(c.correction?.offset??0);rt.end(own);port.remove();
    }
    return offsets;
  })()`);
  expect(result).toEqual([0,1300,0,1200]);
});

test('fling preloading does not expose the trail before the geometric window reaches it', async () => {
  const result=await evaluate(`(() => {
    const port=document.createElement('div');document.querySelector('main').append(port);
    const own=rt.scope(()=>list.vl(port,()=>[1,2,3],i=>i(),()=>{}, {est:100,pw:[300],ph:[200],fixed:[()=>{},()=>{}]}));
    const c=port.$list;c.index.fixed=[300,800];c.geometry={offset:0,port_main:200,cross:300};
    c.realize(false,{velocity:4000});const fling=c.fixed.hidden;
    c.geometry.offset=101;c.realize(false,{});const near=c.fixed.hidden;
    rt.end(own);port.remove();return [fling,near];
  })()`);
  expect(result).toEqual([true,false]);
});
