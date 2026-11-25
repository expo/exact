import {test} from 'bun:test';
import assert from 'node:assert/strict';
import {fixture} from './surface.mjs';
import {hostFixtureFile, glueDeclarations, glueCase, wasmSource} from './glue-fixture.mjs';
import {existsSync} from 'node:fs';
import {resolve} from 'node:path';
import {suiteBrowser} from '../../../scripts/agent-launch.mjs';
import {engineTests} from '../../../scripts/coverage.mjs';
import {decodePng} from '../../../scripts/png.mjs';

const engineTest = await engineTests(['chromium', 'firefox', 'webkit']);
const placementSource = glueDeclarations(hostFixtureFile(process.env.E2B_GPU_SOURCE || new URL('../gpu-glue.js', import.meta.url), 'utf8'),
  ['childFrames', 'PLACEMENT_OPACITY', 'placementStyle', 'placementOpacity', 'restoreChild', 'supplyChildren', 'clipPath', 'placeChildren', 'beforeStyle', 'afterStyle']);
const childrenCase = glueCase(wasmSource, 'children');

// The production placement code over a canvas host in a page: `child()` adds a 40px red label, `layout()` supplies
// and places, and the module puts child i at (10 + 50i, 10) with `factor` as its placement opacity.
async function browserPlacement(engine, run) {
  const root = resolve(import.meta.dir, '../../..');
  const shell = hostFixtureFile(new URL('../index.html', import.meta.url), 'utf8').match(/<style>([\s\S]*?)<\/style>/)[1];
  const server = Bun.serve({hostname:'127.0.0.1', port:0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response(`<style>${shell}
      body{margin:0;background:white} #exact-root{position:relative;isolation:isolate;height:100px}
      canvas{position:absolute;inset:0;width:100%;height:100%}
      @keyframes fade{from{opacity:.6}to{opacity:.2}}
      /* The author's classes, at the specificity the JS target writes them (emit.rs), over the shell's rules. */
      #exact-root#exact-root{
        .label{position:absolute;top:10px;width:40px;height:40px;padding:0;border:0;background:red}
        .authored{opacity:.6} .filtered{filter:opacity(.75)}
        .animated{animation:fade 1000ms linear both paused}
      }
      </style><div id="exact-root"><canvas data-surface></canvas></div>`, {headers:{'content-type':'text/html'}});
    if (path === '/names.js') return new Response('export const sourceTypes={};', {headers:{'content-type':'text/javascript'}});
    const file = [resolve(root, 'host', '.' + path), resolve(root, 'host/web-js', '.' + path), resolve(root, 'host/web', '.' + path)].find(existsSync);
    return file ? new Response(hostFixtureFile(file), {headers:{'content-type':'text/javascript'}}) : new Response(null, {status:404});
  }});
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({viewport:{width:400,height:100}});
    await page.goto(server.url.href);
    await page.evaluate(({source, childrenCase}) => {
      const host = window.host = document.getElementById('exact-root');
      const entry = {host,el:host.querySelector('canvas'),id:1};
      window.factor = .25; window.outcome = 1;
      window.gpu = {
        gpu_children_mode:()=>3, gpu_child_view() {}, gpu_children_count() {},
        gpu_placement:(_,i,out)=>{out.set([1,0,10+i*50,0,1,10,0,0,1,-1]);return window.outcome;},
        gpu_placement_opacity:()=>window.factor,
      };
      const placement = new Function('gpu', 'surfaces', source + ';return {supplyChildren,placeChildren,placementStyle,beforeStyle,afterStyle};')(gpu, new Map([[entry.id, entry]]));
      (globalThis.exact ??= {}).gpu = placement;
      document.head.append(Object.assign(document.createElement('style'), {textContent:placement.placementStyle}));
      window.layout = () => {placement.supplyChildren(entry);placement.placeChildren(entry);};
      window.child = (tag = 'div', cls = '') => {
        const el = document.createElement(tag);
        el.className = `label ${cls}`;el.style.position='absolute';el.style.left = `${10+(host.children.length-1)*50}px`;
        host.append(el);return el;
      };
      // Fade only the child the surface was told is `name`, as a surface that anchors labels by testId does.
      window.fadeNamed = name => {
        const names = [];
        gpu.gpu_child_view = (_,i,told) => {names[i] = told;};
        gpu.gpu_placement_opacity = (_,i) => names[i] === name ? factor : 1;
      };
      // The wasm glue's `children` operation (wasm/commit.js) setting the host's children to `labels`.
      const apply = new Function('op', 'viewFor', 'lists', 'listItem', 'settleValue', `switch (op.op) { ${childrenCase} }`);
      window.setChildren = labels => apply({op:'children',id:0,ids:labels.map((_,i)=>i+1)}, (_,id)=>[host,...labels][id], new Set(), el=>el, ()=>{});
    }, {source:placementSource, childrenCase});
    await run(page);
  } finally {await browser.close();server.stop(true);}
}

async function paintedOpacity(page, expected, x = 30) {
  const {width,data} = decodePng(await page.screenshot());
  for (const [i, opacity] of expected.entries()) {
    const at = (30*width + x+i*50)*4;
    assert.ok(Math.abs(data[at+1] - 255*(1-opacity)) <= 2,
      `child ${i}: painted opacity ${(255-data[at+1])/255}, expected ${opacity}`);
  }
}

for (const engine of ['chromium', 'firefox', 'webkit']) {
  engineTest[engine](`${engine}: placement opacity multiplies classes, control states and shadows, and leaves them authored`, () => browserPlacement(engine, async page => {
    await page.evaluate(async () => {
      const {createInputHandlers} = await import('/input-glue.js');
      createInputHandlers({root: host, views: new Map(), retiredViews: new Set(), ready: () => true, inertAncestor: () => false, dispatch() {}});
      child('div','authored').style.boxShadow='-25px 0 0 0 red';
      child('button').id = 'pressed';child('button').disabled = true;
      // A Contract `button`, whose every property the shell reverts to the browser's (index.html `[data-native]`).
      const native=child('button','authored');native.setAttribute('data-native','');native.setAttribute('data-button-style','bordered');
      layout();
    });
    await page.locator('#pressed').hover();
    await page.mouse.down();
    await page.locator('#pressed').evaluate(el => Promise.all(el.getAnimations().map(animation => animation.finished)));
    await paintedOpacity(page,[.15,.1625,.125,.15]);
    await paintedOpacity(page,[.15],5);
    await page.evaluate(() => {factor=1;layout();});
    await paintedOpacity(page,[.6,.65,.5,.6]);
    await page.evaluate(() => {factor=.25;layout();delete gpu.gpu_placement_opacity;layout();});
    await paintedOpacity(page,[.6,.65,.5,.6]);
    assert.deepEqual(await page.evaluate(() => [...document.querySelectorAll('#exact-root > :not(canvas)')].map(el=>el.style.opacity)),['','','','']);
    await page.mouse.up();
  }));

  engineTest[engine](`${engine}: JS opacity bindings remain authored while placed and after unplacing`, () => browserPlacement(engine, async page => {
    await page.evaluate(async () => {
      const {css} = await import('/rt.js');
      window.label = child();css(label,'opacity','',.6);layout();
      css(label,'opacity','',.8);
    });
    await paintedOpacity(page,[.2]);
    await page.evaluate(() => {layout();});
    await paintedOpacity(page,[.2]);
    await page.evaluate(() => {outcome=0;layout();});
    await paintedOpacity(page,[.8]);
    assert.equal(await page.evaluate(() => label.style.opacity),'0.8');
  }));

  engineTest[engine](`${engine}: placing class-positioned labels keeps their authored frames`, () => browserPlacement(engine, async page => {
    const result = await page.evaluate(() => {
      const labels = [child(), child()];
      for (const el of labels) el.style.removeProperty('position');
      const frames = [];
      gpu.gpu_child_view = (_, i, name, ...frame) => { frames[i] = frame; };
      layout(); layout();
      return {frames, positions: labels.map(el => [el.style.position, getComputedStyle(el).position])};
    });
    assert.deepEqual(result, {frames:[[10,10,40,40],[60,10,40,40]],positions:[['','absolute'],['','absolute']]});
  }));

  engineTest[engine](`${engine}: a placed label follows a conditional position class at a breakpoint`, () => browserPlacement(engine, async page => {
    await page.evaluate(async () => {
      const {commit, scope, sig, write} = await import('/rt.js');
      const {Pc} = await import('/lower.js');
      const el = child(), wide = matchMedia('(min-width: 450px)');
      el.style.removeProperty('position');
      const arm = sig(Number(wide.matches));
      wide.addEventListener('change', () => commit(() => write(arm.n, Number(wide.matches))));
      scope(() => Pc(el, arm, 'label ', 'xa-placement', [['static'], ['absolute']], ['position'],
        (set, [position]) => set(el, 'position', '', () => position)));
      layout();
    });
    const label = page.locator('.label');
    assert.equal(await label.evaluate(el => getComputedStyle(el).position), 'relative');
    await page.setViewportSize({width:500, height:100});
    await page.waitForFunction(() => document.querySelector('.xa-placement-1'));
    assert.equal(await label.evaluate(el => getComputedStyle(el).position), 'absolute');
    await page.setViewportSize({width:400, height:100});
    await page.waitForFunction(() => document.querySelector('.xa-placement-0'));
    assert.equal(await label.evaluate(el => getComputedStyle(el).position), 'relative');
    await page.evaluate(() => {outcome=0;layout();});
    assert.equal(await label.evaluate(el => getComputedStyle(el).position), 'static');
  }));

  engineTest[engine](`${engine}: updating a placed label keeps its placement and restores the latest binding values`, () => browserPlacement(engine, async page => {
    const result = await page.evaluate(async () => {
      const {css} = await import('/rt.js');
      const label = child();
      const styles = () => ['position','transform','transform-origin','z-index','visibility'].map(prop => label.style.getPropertyValue(prop));
      layout();
      for (const [prop, value] of Object.entries({position:'relative', transform:'rotate(7deg)', 'transform-origin':'center', 'z-index':7, visibility:'hidden'})) {
        css(label, prop, '', value);
      }
      const immediate = styles();
      layout();
      const placed = styles();
      window.outcome=0; layout();
      return {immediate, placed, unplaced: styles()};
    });
    assert.deepEqual(result.immediate, result.placed);
    assert.match(result.placed[1], /^matrix3d\(/);
    assert.match(result.placed[2], /^0px 0px(?: 0px)?$/);
    assert.deepEqual(result.placed.slice(3), ['-1','hidden']);
    assert.match(result.unplaced[2], /^center center(?: 0px)?$/);
    assert.deepEqual(result.unplaced.filter((_, i) => i !== 2), ['relative','rotate(7deg)','7','hidden']);
  }));

  engineTest[engine](`${engine}: placement opacity defaults, clamps and leaves a transparent label activatable`, () => browserPlacement(engine, async page => {
    await page.evaluate(() => {
      window.label=child('button');label.style.opacity='.6';label.setAttribute('aria-label','Label');
      window.presses=0;label.onclick=()=>presses++;
      window.reads=0;gpu.gpu_placement_opacity=()=>{reads++;return factor;};
      layout();layout();
    });
    await paintedOpacity(page,[.15]);
    await page.evaluate(() => {factor=0;layout();});
    await paintedOpacity(page,[0]);
    await page.getByRole('button',{name:'Label'}).click();
    assert.equal(await page.evaluate(() => presses),1);
    assert.match(await page.locator('body').ariaSnapshot(),/button "Label"/);
    for (const [factor,expected] of [[NaN,.6],[Infinity,.6],[-Infinity,.6],[-1,0],[2,.6]]) {
      await page.evaluate(value => {window.factor=value;layout();},factor);
      await paintedOpacity(page,[expected]);
    }
    const before=await page.evaluate(() => {factor=.25;layout();outcome=2;const before=reads;layout();return before;});
    assert.equal(await page.evaluate(() => reads),before);
    await page.evaluate(() => {outcome=0;layout();});
    assert.equal(await page.evaluate(() => reads),before);
    await paintedOpacity(page,[.6]);
    await page.evaluate(() => {outcome=1;layout();gpu.gpu_children_mode=()=>0;layout();});
    await paintedOpacity(page,[.6]);
    assert.deepEqual(await page.evaluate(() => [...label.style]),['position','left','opacity']);
  }));

  engineTest[engine](`${engine}: placement opacity multiplies a sought browser fade and a mask the author changes`, () => browserPlacement(engine, async page => {
    await page.evaluate(() => {
      window.label=child('div','animated');
      window.fade=label.getAnimations()[0];fade.currentTime=500;
      window.masked=child('div','authored');masked.style.maskImage='linear-gradient(rgb(0 0 0 / .5) 0 0)';
      layout();
    });
    await paintedOpacity(page,[.1,.075]);
    // No placement pass follows the author's changes: the surface rests.
    await page.evaluate(async () => {
      const {css} = await import('/rt.js');
      fade.currentTime=750;css(masked,'mask-image','','linear-gradient(rgb(0 0 0 / .8) 0 0)');
    });
    await paintedOpacity(page,[.075,.12]);
    await page.evaluate(() => {outcome=0;layout();});
    await paintedOpacity(page,[.3,.48]);
    assert.deepEqual(await page.evaluate(() => [label.getAnimations().length, label.getAnimations()[0] === fade]),[1,true]);
  }));

  // The web's known difference (LLP 1133.039 D8): the author's `filter` wins, so that child is not faded.
  engineTest[engine](`${engine}: an authored filter keeps its transition and is not faded; a filter inside the label is`, () => browserPlacement(engine, async page => {
    await page.evaluate(() => {
      const sought = el => {
        el.style.filter='opacity(.5)';el.style.transition='filter 1000ms linear';
        getComputedStyle(el).filter;el.style.filter='opacity(.8)';
        const transition=el.getAnimations()[0];transition.pause();transition.currentTime=500;return transition;
      };
      window.label=child('div','authored');window.own=sought(label);
      window.outer=child('div','authored');outer.style.background='none';
      window.inner=outer.appendChild(document.createElement('div'));inner.style.cssText='height:100%;background:red';
      window.inside=sought(inner);
      window.fixed=child('div','authored filtered');
    });
    await paintedOpacity(page,[.39,.39,.45]);
    await page.evaluate(() => {layout();});
    await paintedOpacity(page,[.39,.0975,.45]);
    await page.evaluate(() => {factor=.5;layout();own.currentTime=inside.currentTime=750;fixed.classList.remove('filtered');});
    await paintedOpacity(page,[.435,.2175,.3]);
    assert.deepEqual(await page.evaluate(() => [label.getAnimations()[0] === own, inner.getAnimations()[0] === inside, own.playState, inside.playState]),[true,true,'paused','paused']);
    await page.evaluate(() => {outcome=0;layout();own.currentTime=inside.currentTime=250;});
    await paintedOpacity(page,[.345,.345,.6]);
  }));

  engineTest[engine](`${engine}: a faded in-flow child keeps its flow slot, its frame and its siblings`, () => browserPlacement(engine, async page => {
    const before = await page.evaluate(() => {
      window.labels=[child(),child(),child()];
      for (const el of labels) {el.style.position='static';el.style.left='auto';el.style.top='auto';}
      gpu.gpu_placement=(_,i,out)=>{out.set([1,0,10+i*50,0,1,10,0,0,1,i]);return i===2 ? 0 : outcome;};
      window.frames=[];gpu.gpu_child_view=(_,i,name,...frame)=>frames.push([i,...frame]);
      const sibling=labels[2].getBoundingClientRect().y;
      layout();return sibling;
    });
    assert.deepEqual(await page.evaluate(() => labels.slice(0,2).map(el=>{
      const r=el.getBoundingClientRect();return [r.x,r.y,r.width,r.height];
    })),[[10,10,40,40],[60,10,40,40]]);
    await paintedOpacity(page,[.25,.25]);
    assert.equal(await page.evaluate(() => labels[2].getBoundingClientRect().y),before);
    await page.evaluate(() => {layout();factor=1;layout();factor=.5;layout();});
    assert.deepEqual(await page.evaluate(() => frames),[[0,0,0,40,40],[1,0,40,40,40],[2,0,80,40,40]]);
    assert.equal(await page.evaluate(() => labels[2].getBoundingClientRect().y),before);
    assert.equal(await page.evaluate(() => [...host.children].every((el,i) => el === [host.firstElementChild,...labels][i]) && host.children.length === 4),true);
  }));

  engineTest[engine](`${engine}: a fade leaves keyboard focus on the label and animations inside it on their clocks`, () => browserPlacement(engine, async page => {
    await page.evaluate(() => {
      window.label=child('button','authored');label.style.background='none';
      window.inner=label.appendChild(document.createElement('div'));inner.className='animated';inner.style.cssText='height:100%;background:red';
      window.sought=inner.getAnimations()[0];sought.currentTime=500;
      window.running=inner.animate({outlineOffset:['0px','1px']},{duration:1e7});
      factor=1;layout();
      window.blurs=0;label.addEventListener('blur',()=>blurs++);label.focus();
    });
    const started = await page.evaluate(() => running.ready.then(() => running.startTime));
    for (const [factor,outcome,painted] of [[.25,1,.06],[1,1,.24],[.5,1,.12],[.5,0,.24]]) {
      await page.evaluate(([f,o]) => {window.factor=f;window.outcome=o;layout();},[factor,outcome]);
      await paintedOpacity(page,[painted]);
      assert.deepEqual(await page.evaluate(() => [document.activeElement === label, blurs]),[true,0]);
      assert.deepEqual(await page.evaluate(() => [inner.getAnimations().includes(sought), sought.currentTime, inner.getAnimations().includes(running), running.playState, running.startTime]),
        [true,500,true,'running',started]);
    }
  }));

  engineTest[engine](`${engine}: a wasm children update moves a label with its fade`, () => browserPlacement(engine, async page => {
    await page.evaluate(() => {
      window.labels=['a','b','c'].map(name => {const el=child('div','authored');el.setAttribute('data-testid',name);return el;});
      fadeNamed('b');layout();
    });
    await paintedOpacity(page,[.6,.15,.6]);
    // The operation alone, before the commit's placement pass: the label is still where it was placed.
    await page.evaluate(() => {const [a,b,c]=labels;setChildren([b,c,a]);});
    assert.deepEqual(await page.evaluate(() => [...host.children].map(el => el.getAttribute('data-testid'))),[null,'b','c','a']);
    await paintedOpacity(page,[.6,.15,.6]);
    await page.evaluate(() => {layout();});
    await paintedOpacity(page,[.15,.6,.6]);
    await page.evaluate(() => {const [a,b]=labels;setChildren([a,b]);layout();});
    await paintedOpacity(page,[.6,.15]);
    assert.equal(await page.evaluate(() => labels[2].isConnected),false);
  }));

  engineTest[engine](`${engine}: a keyed insertion beside a faded label lands, and the fade stays with its label`, () => browserPlacement(engine, async page => {
    await page.evaluate(async () => {
      const rt = await import('/rt.js');
      window.items = rt.sig(['b']);
      rt.scope(() => rt.each(host, items, item => item(), (p, item) => {
        // Positioned inline, as `child()` does: placement reads a child's inline `position`.
        const el = rt.h(p, 'div', 0, {'data-testid':item()});el.className='label authored';el.style.position='absolute';
      }, true));
      window.prepend = name => rt.commit(() => rt.write(items.n, [name, ...items()]));
      fadeNamed('b');layout();
    });
    await paintedOpacity(page,[.15]);
    await page.evaluate(() => {prepend('a');});
    assert.deepEqual(await page.evaluate(() => [...host.children].map(el => el.getAttribute('data-testid'))),[null,'a','b']);
    await page.evaluate(() => {layout();});
    await paintedOpacity(page,[.6,.15]);
  }));
}

test('host-composited children use local homographies, depth order, explicit hiding and retirement', async()=>{
  const f=await fixture(), host=f.create(1), frames=[];
  const child=(x,y,w,h)=>Object.assign(new f.Element('child'),{hasAttribute:()=>false,offsetLeft:x,offsetTop:y,offsetWidth:w,offsetHeight:h,inert:false});
  const hud=child(24,24,100,20), a=child(10,40,100,50), b=child(10,90,100,50);
  host.children=[hud,a,b];
  let hidden=false, none=false;
  Object.assign(f.gpu,{
    gpu_children_mode:()=>3,
    gpu_child_view:(id,i,name,...frame)=>frames.push([i,...frame]), gpu_children_count:(id,n)=>frames.push(['count',n]),
    gpu_dirty:()=>true,
    gpu_placement:(id,i,out)=>{ if(i===0 || none)return 0; if(i===1&&hidden)return 2;out.set([2,0,100,0,2,200,0.001,0,1,i===1?-2:-5]);return 1; },
  });
  f.exact.gpu.layout();f.frame();
  assert.deepEqual(frames,[[0,24,24,100,20],[1,10,40,100,50],[2,10,90,100,50],['count',3]]);
  assert.deepEqual(hud.style,{},'None leaves the HUD at its kernel frame');
  const matrix=a.style.transform.slice(9,-1).split(',').map(Number);
  const map=(x,y)=>{const w=matrix[3]*x+matrix[7]*y+matrix[15];return [a.offsetLeft+(matrix[0]*x+matrix[4]*y+matrix[12])/w,a.offsetTop+(matrix[1]*x+matrix[5]*y+matrix[13])/w];};
  assert.deepEqual(map(0,0),[100,200]);
  const p=map(100,50);assert.ok(Math.abs(p[0]-300/1.1)<0.0001&&Math.abs(p[1]-300/1.1)<0.0001);
  assert.equal(a.style.transformOrigin,'0 0');assert.ok(Number(a.style.zIndex)>Number(b.style.zIndex));assert.ok(Number(a.style.zIndex)<0);
  hidden=true;f.frame();assert.equal(a.style.visibility,'hidden');assert.equal(a.inert,true);
  hidden=false;f.frame();assert.equal(a.inert,false);
  none=true;f.frame();assert.equal(a.style.transform,undefined);assert.equal(a.inert,false);
  host.children=[hud];f.exact.gpu.layout();assert.deepEqual(frames.at(-1),['count',1]);
});

for (const fail of [false,true]) test(`placement frames survive module replacement; failed=${fail}`, async()=>{
  const frames=[];
  const f=await fixture({nextGpu:{
    gpu_children_mode:()=>fail ? 0 : 3,
    gpu_child_view:(id,i,name,...frame)=>frames.push([i,...frame]), gpu_children_count:()=>{},
    gpu_placement:(id,i,out)=>{out.set([1,0,200,0,1,100,0,0,1,-1]);return 1;},
    gpu_render:()=>fail?2:0,
  }});
  const host=f.create(1), child=Object.assign(new f.Element('child'),{hasAttribute:()=>false,offsetLeft:10,offsetTop:20,offsetWidth:100,offsetHeight:50,inert:false});
  host.children=[child];
  Object.assign(f.gpu,{
    gpu_children_mode:()=>3,gpu_child_view:()=>{},gpu_children_count:()=>{},gpu_dirty:()=>true,
    gpu_placement:(id,i,out)=>{out.set([1,0,100,0,1,100,0,0,1,-1]);return 1;},
  });
  f.exact.gpu.layout();f.frame();
  const before=child.style.transform;
  if(fail){await assert.rejects(f.exact.gpu.swap(1),/render/);assert.equal(child.style.transform,before);}
  else {await f.exact.gpu.swap(1);assert.deepEqual(frames,[[0,10,20,100,50]]);assert.notEqual(child.style.transform,before);}
});

test('paused placement survives authored style updates and restores latest styles', async()=>{
  const f=await fixture(), host=f.create(1);
  const child=Object.assign(new f.Element('child'),{hasAttribute:()=>false,offsetLeft:10,offsetTop:20,offsetWidth:100,offsetHeight:50,inert:false});
  host.children=[child];let each=true, none=false;const counts=[];
  Object.assign(f.gpu,{
    gpu_children_mode:()=>each ? 3 : 0,gpu_child_view:()=>{},gpu_children_count:(_,n)=>counts.push(n),gpu_dirty:()=>true,
    gpu_placement:(_,i,out)=>{out.set([1,0,200,0,1,100,0,0,1,-1]);return none?0:1;},
    gpu_placement_opacity:()=>.25,
  });
  f.exact.gpu.layout();f.frame();
  assert.equal(child.style['--exact-placement-opacity'],'0.25');
  // Model cssText replacement, then drive the real apply/applyBatch style operation.
  Object.defineProperty(child.style,'cssText',{set(css) {
    for(const key of Object.keys(this)) delete this[key];
    for(const declaration of css.split(';').filter(Boolean)) {
      const [key,value]=declaration.split(':'); this[key.replace(/-([a-z])/g,(_,c)=>c.toUpperCase())]=value;
    }
  }});
  f.exact.views.set(2,child);
  child.offsetLeft=40;child.offsetTop=60;
  f.applyBatch({ops:[{op:'style',id:2,css:'position:absolute;visibility:visible;z-index:7;transform:rotate(3deg);transform-origin:center;opacity:0.8'}]});
  const matrix=child.style.transform.slice(9,-1).split(',').map(Number);
  assert.equal(matrix[12]+child.offsetLeft,200);assert.equal(matrix[13]+child.offsetTop,100);
  assert.ok(child.style.transform.startsWith('matrix3d('),'reapplied without a frame');
  assert.equal(Number(child.style.opacity),0.8);
  assert.equal(child.style['--exact-placement-opacity'],'0.25','the fade returns with the placement');
  none=true;f.frame();
  assert.deepEqual(child.style,{position:'absolute',visibility:'visible',zIndex:'7',transform:'rotate(3deg)',transformOrigin:'center',opacity:'0.8'});
  each=false;f.exact.gpu.layout();
  assert.equal(counts.at(-1),0);
});

test('draw hook inherits resize generation and advances on equal-paced callbacks',async()=>{
  const f=await fixture(), draws=[];f.create(1);let dirty=true;
  Object.assign(f.gpu,{gpu_dirty:()=>dirty,gpu_render:()=>{dirty=false;return 0;}});
  f.exact.drawCallback=(raw,paced,generation)=>draws.push([raw,paced,generation]);
  f.frame();dirty=true;await f.exact.gpu.settled();dirty=true;f.frame();
  assert.deepEqual(draws,[[0,0,1],[0,0,1],[0,0,2]]);
});

test('replacement without Placed restores the current authored style',async()=>{
  const f=await fixture({nextGpu:{gpu_children_mode:()=>0,gpu_children_count:()=>{}}}),host=f.create(1);
  const child=Object.assign(new f.Element('child'),{hasAttribute:()=>false,offsetLeft:0,offsetTop:0,offsetWidth:100,offsetHeight:50,inert:false});
  child.style.transform='rotate(3deg)';host.children=[child];
  Object.assign(f.gpu,{gpu_children_mode:()=>3,gpu_child_view:()=>{},gpu_children_count:()=>{},gpu_dirty:()=>true,
    gpu_placement:(_,i,out)=>{out.set([1,0,200,0,1,100,0,0,1,-1]);return 1;}});
  f.exact.gpu.layout();f.frame();assert.ok(child.style.transform.startsWith('matrix3d'));
  await f.exact.gpu.swap(1);
  assert.equal(child.style.transform,'rotate(3deg)');
});

test('Contract names follow reorder, rename, removal and module replacement', async()=>{
  const delivered=[], staged=[];
  const f=await fixture({nextGpu:{gpu_children_mode:()=>3,gpu_child_view:(_,i,name,...frame)=>staged.push([i,name,...frame]),gpu_children_count:()=>{},gpu_placement:()=>0}});
  const host=f.create(1);
  const child=name=>Object.assign(new f.Element('child'),{name,hasAttribute:()=>false,getAttribute(key){return key==='data-testid'?this.name:null;},offsetLeft:0,offsetTop:0,offsetWidth:100,offsetHeight:50});
  const a=child('標識 🏮'),b=child('pull');host.children=[a,b];
  Object.assign(f.gpu,{gpu_children_mode:()=>3,gpu_child_view:(_,i,name,...frame)=>delivered.push([i,name,...frame]),gpu_children_count:()=>{},gpu_placement:()=>0});
  f.exact.gpu.layout();
  assert.deepEqual(delivered,[[0,'標識 🏮',0,0,100,50],[1,'pull',0,0,100,50]]);
  delivered.length=0;host.children=[b,a];f.exact.gpu.layout();
  assert.deepEqual(delivered.map(r=>r.slice(0,2)),[[0,'pull'],[1,'標識 🏮']]);
  delivered.length=0;a.name='sign';f.exact.gpu.layout();
  assert.deepEqual(delivered,[[1,'sign',0,0,100,50]],'rename with identical frame must be delivered');
  delivered.length=0;f.exact.gpu.layout();assert.deepEqual(delivered,[],'unchanged metadata stays quiet');
  await f.exact.gpu.swap(1);
  assert.deepEqual(staged.map(r=>r.slice(0,2)),[[0,'pull'],[1,'sign']]);
  host.children=[a];f.exact.gpu.layout();
  assert.deepEqual(staged.at(-1).slice(0,2),[0,'sign']);
});

test('placement clip rows cut the child to the planes Linux clips its composite to', async()=>{
  const f=await fixture(), host=f.create(1);
  const child=Object.assign(new f.Element('child'),{hasAttribute:()=>false,offsetLeft:10,offsetTop:20,offsetWidth:100,offsetHeight:50,inert:false});
  host.children=[child];
  // x >= 30 and y <= 40 in the child's points; later rows that cut nothing, a module that writes ten floats, and rows that cut all.
  let rows=[1,0,-30,0,-1,40];
  Object.assign(f.gpu,{
    gpu_children_mode:()=>3,gpu_child_view:()=>{},gpu_children_count:()=>{},gpu_dirty:()=>true,
    gpu_placement:(_,i,out)=>{out.set([1,0,0,0,1,0,0,0,1,-1]);if(rows)out.set(rows,10);return 1;},
  });
  f.exact.gpu.layout();f.frame();
  assert.equal(child.style.clipPath,'polygon(30px 40px,30px 0px,100px 0px,100px 40px)');
  rows=[0,0,1,0,0,1];f.frame();
  assert.ok(!child.style.clipPath,'rows that cut nothing leave no clip');
  rows=[1,0,-30,0,-1,40];f.frame();rows=null;f.frame();
  assert.ok(!child.style.clipPath,'a ten-float module writes no rows');
  rows=[-1,0,-1,0,0,1];f.frame();
  assert.equal(child.style.clipPath,'polygon(0px 0px)','rows that cut everything leave nothing');
});
