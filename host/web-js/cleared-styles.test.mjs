import { expect, test } from 'bun:test';
import { EXE } from '../../scripts/paths.mjs';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';
import { withLook } from '../web/look.mjs';

const ROOT = resolve(import.meta.dir, '../..');
const CASES = {
  unsaid: '',
  transparent: 'background-color="transparent"',
  bound: 'background-color=fill',
  'bound-class': 'class=Paint background-color=fill',
  lowered: 'background-color=(mode == "color" ? "#000000" : "")',
  removed: 'background-color=(mode == "color" ? "#000000" : none)',
  class: 'class=Paint',
};
const PRESENTATIONS = ['fullscreen', 'modal'];
// An app has one navigation root, its view's root, so each case is its own app.
const coverSource = (presentation, name) => `style Paint
  background-color="#123456"

component App
  state fill = ""
  state mode = "empty"
  action color
    fill = "#000000"
    mode = "color"
  action clear
    fill = ""
    mode = "empty"
  action spaces
    fill = " \\t\\n"
    mode = "spaces"
  view
    main navigationKey="cover" navigationBack="" position="relative" width=180 height=120
      button "Color" press=color testId="color"
      button "Clear" press=clear testId="clear"
      button "Whitespace" press=spaces testId="spaces"
      text mode testId="mode"
      column navigationKey="home" testId="home-${presentation}-${name}" position="absolute" inset=0 background-color="#eeccaa"
        text "Home"
      column navigationKey="cover" navigationPresentation="${presentation}" testId="${presentation}-${name}" position="absolute" inset=0 ${CASES[name]}
        text "Cover"
`;

async function fixture(runtime, run, contract, files = {}, engine = 'chromium') {
  const dir = mkdtempSync(resolve(ROOT, 'target/cleared-styles-'));
  let server, browser, session;
  try {
    const source = resolve(dir, 'app.contract');
    writeFileSync(source, contract);
    for (const [name, text] of Object.entries(files)) writeFileSync(resolve(dir, name), text);
    const built = spawnSync(resolve(ROOT, `target/debug/exact-web-js${EXE}`), ['js', source, '-o', dir], { encoding: 'utf8' });
    expect(built.status, built.stderr).toBe(0);
    if (runtime === 'wasm') {
      session = await open({ host: 'web', plan: resolve(dir, 'app.plan'), webDist: resolve(process.env.EXACT_WEB_DIST) });
      await run(expression => session.carrier.evaluate(expression));
    } else {
      const shell = withLook(readFileSync(resolve(ROOT, 'host/web/index.html'), 'utf8'));
      const style = shell.match(/<style>([\s\S]*?)<\/style>/)[1];
      server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
        const path = new URL(request.url).pathname.slice(1);
        if (!path) return new Response(`<style>${style}</style><link rel="stylesheet" href="/app.css">
          <div id="exact-root"></div><script type="module">
          import app from '/app.js';
          window.smoothingWrites = [];
          const set = CSSStyleDeclaration.prototype.setProperty;
          CSSStyleDeclaration.prototype.setProperty = function(p, v, ...rest) {
            if (p === '-moz-osx-font-smoothing') window.smoothingWrites.push(v);
            return set.call(this, p, v, ...rest);
          };
          app();window.ready=true;</script>`,
        { headers: { 'content-type': 'text/html' } });
        const bases = path.startsWith('web/') ? [resolve(ROOT, 'host')] : [dir, import.meta.dir, resolve(ROOT, 'host/web')];
        const file = bases.map(base => resolve(base, path)).find(existsSync);
        return file ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
      } });
      browser = await suiteBrowser(engine);
      const page = await browser.newPage();
      await page.goto(server.url.href);
      await page.waitForFunction(() => window.ready);
      await run(expression => page.evaluate(expression));
    }
  } finally {
    await session?.close(); await browser?.close(); server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}

const read = id => `(() => {
  const route = document.querySelector('[data-testid="${id}"]'), home = document.querySelector('[data-testid="home-${id}"]');
  return [getComputedStyle(route).backgroundColor, getComputedStyle(home).visibility];
})()`;

const MAPPED = {
  tracks: ['grid-template-columns', '20px 1fr', '40px 60px'],
  rows: ['grid-template-rows', '20px 1fr', '40px 60px'],
  placement: ['grid-column', '2 / 3', '3 / 4'],
  row: ['grid-row', '2 / 3', '3 / 4'],
  flow: ['grid-auto-flow', 'column', 'row dense'],
  justify: ['justify-items', 'center', 'start'],
  z: ['z-index', '2', '3'],
  fallback: ['position-try-fallbacks', 'flip-block', 'flip-inline'],
  scope: ['timeline-scope', '--local', '--class'],
  smoothing: ['-webkit-font-smoothing', 'antialiased', 'antialiased'],
  backdrop: ['backdrop-filter', 'blur(2px)', 'blur(4px)'],
  transform: ['transform', 'translate(2 3)', 'translate(4 5)'],
  placeholder: ['placeholder-color', '#123456', '#abcdef'],
  accent: ['accent-color', '#123456', '#abcdef'],
  slop: ['-exact-hit-slop-top', '2px', '4px'],
};
const MAPPED_SOURCE = `${Object.entries(MAPPED).map(([name, [property, , fallback]]) => `style C${name}
${name === 'placeholder' ? `  ::placeholder\n    color=${JSON.stringify(fallback)}` : `  ${property}=${JSON.stringify(fallback)}`}`).join('\n\n')}

keyframes Pulse
  from opacity=0
  to opacity=1

component App
${Object.keys(MAPPED).map(name => `  state ${name} = ${name === 'z' ? '2' : '""'}`).join('\n')}
  state timeline = ""
  state animation = ""
  state mode = "empty"
  action set
${Object.entries(MAPPED).map(([name, [, value]]) => `    ${name} = ${name === 'z' ? '2' : JSON.stringify(value)}`).join('\n')}
    mode = "set"
  action clear
${Object.keys(MAPPED).filter(name => name !== 'z').map(name => `    ${name} = ""`).join('\n')}
    mode = "empty"
  action spaces
${Object.keys(MAPPED).filter(name => name !== 'z').map(name => `    ${name} = " \\t\\n"`).join('\n')}
    mode = "spaces"
  action timelineOn
    timeline = "--local"
    animation = "Pulse 3600s"
    mode = "timeline"
  action timelineOff
    timeline = ""
    animation = ""
    mode = "running"
  action timelineUnset
    timeline = "unset"
    mode = "running"
  action animate
    animation = "Pulse 3601s"
    mode = "animated"
  view
    column
      button "Set" press=set testId="set"
      button "Clear" press=clear testId="clear"
      button "Whitespace" press=spaces testId="spaces"
      button "Timeline" press=timelineOn testId="timelineOn"
      button "Run" press=timelineOff testId="timelineOff"
      button "Unset timeline" press=timelineUnset testId="timelineUnset"
      button "Animate" press=animate testId="animate"
      text mode testId="mode"
${Object.entries(MAPPED).map(([name, [property]]) => {
  if (name === 'placeholder') return `      input class=C${name} testId="placeholder"\n        ::placeholder\n          color=placeholder`;
  const tag = name === 'fallback' ? 'view popover="auto"' : name === 'transform' ? 'svg' : 'view';
  const binding = name === 'z' ? '(mode == "set" ? z : mode == "spaces" ? " \\t" : "")' : name;
  return `      ${tag} class=C${name} ${property}=${binding} testId=${JSON.stringify(name)}`;
}).join('\n')}
      view animation="Pulse 3600s" animation-timeline=timeline testId="timeline"
      view animation=animation animation-timeline="--local" testId="animation"
      view -exact-press-scale=0.9 animation=animation animation-timeline="--local" testId="pressed"
      view animation=animation animation-timeline=timeline testId="shared"
      view -exact-press-scale=0.9 animation=animation animation-timeline=timeline testId="shared-pressed"
      view animation=animation animation-timeline="unset" testId="unset-animation"
`;

const readMapped = `(() => {
  const properties = ${JSON.stringify(Object.fromEntries(Object.entries(MAPPED).map(([name, [property]]) =>
    [name, property === 'placeholder-color' ? '--exact-placeholder-color' : property === '-exact-hit-slop-top' ? '--exact-hit-slop-top' : property])))};
  const values = Object.fromEntries(Object.entries(properties).map(([name, property]) => {
    const e = document.querySelector('[data-testid="' + name + '"]');
    return [name, [e.style.getPropertyValue(property), getComputedStyle(e).getPropertyValue(property)]];
  }));
  for (const name of ['timeline', 'animation', 'pressed', 'shared', 'shared-pressed', 'unset-animation']) {
    const e = document.querySelector('[data-testid="' + name + '"]');
    values[name] = [e.style.animationPlayState, getComputedStyle(e).animationPlayState, e.getAnimations().map(a => a.playState), getComputedStyle(e).animationTimeline];
  }
  values.accent.push(document.querySelector('[data-testid="accent"]').style.getPropertyValue('--exact-accent'));
  return values;
})()`;

async function action(evaluate, id, mode) {
  await evaluate(`document.querySelector('[data-testid="${id}"]').click()`);
  await evaluate(`new Promise((ok, fail) => {
    const deadline = performance.now() + 10000;
    const ready = () => {
      if (document.querySelector('[data-testid="mode"]').textContent === ${JSON.stringify(mode)}) return ok();
      if (performance.now() >= deadline) return fail(new Error('the style action did not commit'));
      requestAnimationFrame(ready);
    }; ready();
  })`);
}

// A bound timeline name holds its animations. This plan has no drag timeline, so the JS target writes the name as
// CSS's own row (LLP 1057.003 D5: the browser's timeline, none here, which holds them); the wasm target pauses them
// for the drag code, which finds no source.
const bound = (runtime, mapped) => runtime === 'JS' ? [mapped[1], mapped[3]] : [mapped[1]];
const held = runtime => runtime === 'JS' ? ['running', '--local'] : ['paused'];

for (const runtime of ['JS', 'wasm']) {
  const check = need('fixture', runtime === 'wasm' && !process.env.EXACT_WEB_DIST ? 'name a wasm web build with EXACT_WEB_DIST' : null);
  check(`${runtime}: authored auto, blank and invalid z-index override the sheet's integer`, () => fixture(runtime, async evaluate => {
    await evaluate(`(() => {
      for (const id of ['pushed', 'literal', 'unsaid']) document.querySelector('[data-testid="' + id + '"]').setAttribute('data-exact-in-sheet', '');
    })()`);
    for (const [id, mode, expected] of [[null, 'auto', 'auto'], ['raised', 'raised', '2'], ['invalid', 'invalid', 'auto'],
      ['autoZ', 'auto', 'auto'], ['invalid', 'invalid', 'auto'], ['zero', 'zero', '0'], ['empty', 'empty', 'auto'],
      ['raised', 'raised', '2'], ['spaces', 'spaces', 'auto'], ['zero', 'zero', '0'], ['autoZ', 'auto', 'auto']]) {
      if (id) await action(evaluate, id, mode);
      const values = await evaluate(`(() => Object.fromEntries(['modal', 'pushed', 'literal', 'unsaid'].map(id => {
        const e = document.querySelector('[data-testid="' + id + '"]');
        return [id, [getComputedStyle(e).zIndex, e.getAttribute('data-exact-zi'), e.style.zIndex]];
      })))()`);
      for (const name of ['modal', 'pushed']) {
        expect(values[name][0], `${mode}: ${name}`).toBe(expected);
        if (runtime === 'JS') expect(values[name][1], `${mode}: ${name} paint fact`).toBe(expected === 'auto' ? null : expected);
        if (mode === 'empty' || mode === 'spaces') expect(values[name][2], `${mode}: ${name} authored clear`).toBe('unset');
      }
      expect(values.literal[0]).toBe('auto');
      expect(values.unsaid[0]).toBe('1');
    }
  }, `component App
  state mode = "auto"
  state fraction = 2.5
  action raised
    mode = "raised"
  action invalid
    mode = "invalid"
  action zero
    mode = "zero"
  action autoZ
    mode = "auto"
  action empty
    mode = "empty"
  action spaces
    mode = "spaces"
  view
    column navigationKey="cover" navigationBack=""
      column navigationKey="cover" navigationPresentation="modal" z-index=(mode == "raised" ? 2 : mode == "zero" ? 0 : mode == "invalid" ? fraction : mode == "empty" ? "" : mode == "spaces" ? " \\t\\n" : "auto") testId="modal"
        text mode testId="mode"
        button "Raised" press=raised testId="raised"
        button "Invalid" press=invalid testId="invalid"
        button "Zero" press=zero testId="zero"
        button "Auto" press=autoZ testId="autoZ"
        button "Empty" press=empty testId="empty"
        button "Whitespace" press=spaces testId="spaces"
        view transition="translate -exact-spring(300, 30, 1)" z-index=(mode == "raised" ? 2 : mode == "zero" ? 0 : mode == "invalid" ? fraction : mode == "empty" ? "" : mode == "spaces" ? " \\t\\n" : "auto") testId="pushed"
        view z-index="auto" testId="literal"
        view testId="unsaid"
`), 120000);

  check(`${runtime}: an unset timeline leaves its animations running`, () => fixture(runtime, async evaluate => {
    await action(evaluate, 'timelineOn', 'timeline');
    const active = await evaluate(readMapped);
    expect(bound(runtime, active.timeline)).toEqual(held(runtime));
    expect(active['unset-animation'][1]).toBe('running');
    for (const id of ['timelineUnset', 'timelineOff']) {
      await action(evaluate, id, 'running');
      expect((await evaluate(readMapped)).timeline[1]).toBe('running');
      await action(evaluate, 'animate', 'animated');
      const updated = await evaluate(readMapped);
      for (const name of ['timeline', 'shared', 'shared-pressed', 'unset-animation']) expect(updated[name][1], name).toBe('running');
    }
  }, MAPPED_SOURCE), 120000);

  check(`${runtime}: a cleared authored inset keeps the sheet's opposite inset physical`, () => fixture(runtime, async evaluate => {
    for (const [id, mode] of [[null, 'empty'], ['set', 'set'], ['clear', 'empty'], ['spaces', 'spaces'], ['remove', 'removed']]) {
      if (id) await action(evaluate, id, mode);
      const actual = await evaluate(`(() => Object.fromEntries(['bound', 'conditional', 'plain'].map(id => {
        const s = getComputedStyle(document.querySelector('[data-testid="' + id + '"]'));
        return [id, [s.paddingLeft, s.paddingRight]];
      })))()`);
      for (const name of ['bound', 'conditional']) {
        expect(actual[name], `${mode}: ${name}`).toEqual(mode === 'removed' ? ['0px', actual.plain[1]] : [actual.plain[1], mode === 'set' ? '12px' : '0px']);
      }
    }
  }, `component App
  state p = ""
  state mode = "empty"
  state shown = true
  action set
    p = "12px"
    mode = "set"
  action clear
    p = ""
    mode = "empty"
  action spaces
    p = " \\t"
    mode = "spaces"
  action remove
    mode = "removed"
  view
    column direction="rtl"
      text mode testId="mode"
      button "Set" press=set testId="set"
      button "Clear" press=clear testId="clear"
      button "Spaces" press=spaces testId="spaces"
      button "Remove" press=remove testId="remove"
      list appearance="auto"
        section
          row padding-right=(mode == "removed" ? none : p) testId="bound"
            image "symbol:person"
            text "Bound"
          row padding-right=(mode == "removed" ? none : p) testId="conditional"
            if shown
              image "symbol:person"
            text "Conditional"
          row testId="plain"
            image "symbol:person"
            text "Plain"
`), 120000);

  check(`${runtime}: an empty literal family clears a class and inherits from its parent`, () => fixture(runtime, async evaluate => {
    await evaluate(`(() => {
      document.head.appendChild(document.createElement('style')).textContent = '#exact-root .authored-family{font-family:monospace}';
      for (const id of ['literal', 'spaces', 'choice', 'prop']) document.querySelector('[data-testid="' + id + '"]').classList.add('authored-family');
    })()`);
    const read = `(() => Object.fromEntries(['baseline', 'literal', 'spaces', 'choice', 'prop'].map(id =>
      [id, getComputedStyle(document.querySelector('[data-testid="' + id + '"]')).fontFamily])))()`;
    for (const [id, mode] of [[null, 'empty'], ['set', 'set'], ['clear', 'empty']]) {
      if (id) await action(evaluate, id, mode);
      const actual = await evaluate(read);
      for (const name of ['literal', 'spaces', 'prop']) expect(actual[name], name).toBe(actual.baseline);
      expect(actual.choice).toBe(mode === 'set' ? 'Georgia' : actual.baseline);
    }
  }, `style Family
  font-family="monospace"

component App
  state mode = "empty"
  action set
    mode = "set"
  action clear
    mode = "empty"
  view
    column font-family="serif"
      text mode testId="mode"
      button "Set" press=set testId="set"
      button "Clear" press=clear testId="clear"
      text "Baseline" testId="baseline"
      text "Literal" class=Family font-family="" testId="literal"
      text "Spaces" class=Family font-family=" \\t" testId="spaces"
      text "Choice" class=Family font-family=(mode == "set" ? "Georgia" : "") testId="choice"
      Fam(f="")

component Fam
  props
    f: string
  view
    text "Prop" class=Family font-family=f testId="prop"
`), 120000);

  check(`${runtime}: an empty bound background clears routes over authored classes and the neutral sheet`, async () => {
    const modes = ['empty', 'color', 'empty', 'spaces', 'color', 'empty'], snapshots = modes.map(() => ({}));
    for (const p of PRESENTATIONS) for (const name of Object.keys(CASES)) await fixture(runtime, async evaluate => {
      for (const [i, mode] of modes.entries()) {
        if (i) await evaluate(`document.querySelector('[data-testid="${mode === 'empty' ? 'clear' : mode}"]').click()`);
        await evaluate(`new Promise((ok, fail) => {
          const deadline = performance.now() + 10000;
          const ready = () => {
            if (document.querySelector('[data-testid="mode"]').textContent === ${JSON.stringify(mode)}) return ok();
            if (performance.now() >= deadline) return fail(new Error('the style action did not commit'));
            requestAnimationFrame(ready);
          }; ready();
        })`);
        snapshots[i][`${p}-${name}`] = await evaluate(read(`${p}-${name}`));
      }
      if (runtime === 'JS' && p === 'fullscreen' && name === 'lowered') {
        expect(await evaluate(`[...document.querySelector('[data-testid="fullscreen-lowered"]').classList].some(name => name.startsWith('xa'))`)).toBe(true);
        expect(await evaluate(`(async () => {
          const { css, cssText } = await import('/rt.js');
          const el = document.createElement('div'); el.style.color = 'red';
          const values = [];
          for (const value of ['', null, ' \\t', undefined, '']) {
            css(el, 'color', '', value); values.push(el.style.color);
          }
          return { values, tokens: ['', ' \\t', null].map(v => cssText('--exact-said-font-weight', '', v)) };
        })()`)).toEqual({ values: ['unset', '', 'unset', '', 'unset'], tokens: ['', ' \t', null] });
      }
    }, coverSource(p, name));
    for (const [i, mode] of modes.entries()) {
      const actual = snapshots[i];
      for (const p of PRESENTATIONS) {
        for (const name of ['bound', 'bound-class', 'lowered']) {
          expect([mode, p, name, actual[`${p}-${name}`]]).toEqual([mode, p, name, [mode === 'color' ? 'rgb(0, 0, 0)' : 'rgba(0, 0, 0, 0)', 'visible']]);
        }
        expect(actual[`${p}-transparent`]).toEqual(['rgba(0, 0, 0, 0)', 'visible']);
        expect(actual[`${p}-class`]).toEqual(['rgb(18, 52, 86)', 'visible']);
      }
      expect(actual['fullscreen-unsaid']).toEqual(['rgba(1, 2, 3, 0)', 'hidden']);
      expect(actual['modal-unsaid'][0]).not.toBe('rgba(0, 0, 0, 0)');
      expect(actual['modal-unsaid'][1]).toBe('visible');
      for (const p of PRESENTATIONS) {
        expect(actual[`${p}-removed`]).toEqual(mode === 'color' ? ['rgb(0, 0, 0)', 'visible'] : actual[`${p}-unsaid`]);
      }
    }
    expect(snapshots[0]).toEqual(snapshots[2]);
    expect(snapshots[0]).toEqual(snapshots[3]);
    expect(snapshots[0]).toEqual(snapshots[5]);
  }, 240000);

  check(`${runtime}: mapped clears preserve class precedence, companion absence and running animations`, () => fixture(runtime, async evaluate => {
    await evaluate(`(() => {
      const sheet = document.head.appendChild(document.createElement('style'));
      sheet.textContent = ${JSON.stringify(Object.entries(MAPPED).map(([name, [property, , value]]) =>
        `#exact-root .authored-${name}{${name === 'placeholder' ? '--exact-placeholder-color' : name === 'slop' ? '--exact-hit-slop-top' : property}:${name === 'transform' ? 'translate(4px, 5px)' : value}}`).join('\n'))};
      for (const name of ${JSON.stringify(Object.keys(MAPPED))}) document.querySelector('[data-testid="' + name + '"]').classList.add('authored-' + name);
    })()`);
    for (const [id, mode] of [[null, 'empty'], ['set', 'set'], ['clear', 'empty'], ['spaces', 'spaces']]) {
      if (runtime === 'JS' && id) await evaluate(`import('/rt.js').then(m => { m.journal.length = 0; window.smoothingWrites.length = 0; })`);
      if (id) await action(evaluate, id, mode);
      const actual = await evaluate(readMapped);
      if (mode === 'set') {
        expect(actual.tracks).toEqual(['20px 1fr', '20px 1fr']);
        expect(actual.z).toEqual(['2', '2']);
        expect(actual.backdrop).toEqual(['blur(2px)', 'blur(2px)']);
      } else {
        const names = ['tracks', 'rows', 'placement', 'row', 'flow', 'justify', 'z', 'fallback', 'backdrop', 'transform', 'accent', 'placeholder', 'scope', 'smoothing', 'slop'];
        expect(Object.fromEntries(names.map(name => [name, actual[name][0]]))).toEqual(Object.fromEntries(names.map(name =>
          [name, name === 'scope' ? '' : name === 'placeholder' ? 'initial' : 'unset'])));
        expect(actual.placeholder[0]).toBe('initial');
        expect(actual.accent[2]).toBe('unset');
        expect(actual.scope).toEqual(['', '--class']);
        expect(actual.smoothing[0]).toBe('unset');
        if (runtime === 'JS') {
          expect(await evaluate(`import('/rt.js').then(m => m.journal.filter(l => l.includes('unset ')))`)).toEqual([]);
          // A clear unsets Firefox's spelling too, once (whitespace after a clear writes nothing new); Chrome drops it.
          expect(await evaluate('window.smoothingWrites')).toEqual(id === 'spaces' ? [] : ['unset']);
        }
      }
      expect(actual.timeline[1]).toBe('running');
      if (runtime === 'JS') expect(actual.timeline).toEqual(['', 'running', ['running'], 'auto']);
    }
    await action(evaluate, 'timelineOn', 'timeline');
    expect(bound(runtime, (await evaluate(readMapped)).timeline)).toEqual(held(runtime));
    await action(evaluate, 'timelineUnset', 'running');
    expect((await evaluate(readMapped)).timeline[1]).toBe('running');
    await action(evaluate, 'timelineOn', 'timeline');
    await action(evaluate, 'timelineOff', 'running');
    const cleared = await evaluate(readMapped);
    expect(cleared.timeline[1]).toBe('running');
    if (runtime === 'JS') expect(cleared.timeline).toEqual(['', 'running', ['running'], 'auto']);
    for (const name of ['animation', 'pressed']) {
      expect(cleared[name][2]).toEqual([]);
      if (runtime === 'JS') expect(cleared[name][0]).toBe('');
    }
  }, MAPPED_SOURCE), 120000);

  check(`${runtime}: a cleared accent releases a mode boundary's token marker`, () => fixture(runtime, async evaluate => {
    for (const [id, mode] of [[null, 'color'], ['clear', 'empty'], ['color', 'color'], ['spaces', 'spaces']]) {
      if (id) await action(evaluate, id, mode);
      const values = await evaluate(`(() => {
        const e = document.querySelector('[data-testid="accent"]');
        return { marker: e.getAttribute('data-x-accent'), own: e.style.getPropertyValue('--exact-accent'),
          color: getComputedStyle(e).accentColor, baseline: getComputedStyle(document.querySelector('[data-testid="baseline"]')).accentColor };
      })()`);
      if (runtime === 'JS') expect([values.marker, values.own]).toEqual([mode === 'color' ? '--x-accent-mix' : null, '']);
      expect(values.color).toBe(mode === 'color' ? 'rgb(0, 0, 0)' : values.baseline);
    }
  }, `component App
  state fill = "accent.mix"
  state mode = "color"
  action clear
    fill = ""
    mode = "empty"
  action color
    fill = "accent.mix"
    mode = "color"
  action spaces
    fill = " \\t"
    mode = "spaces"
  view
    column
      view testId="baseline"
      column accent-color=fill -exact-mode="ink=plain" testId="accent"
        text mode testId="mode"
        button "Clear" press=clear testId="clear"
        button "Color" press=color testId="color"
        button "Spaces" press=spaces testId="spaces"
`, { 'app.tamber': 'tamber t\n\naxes\n  ink = tint | plain\n\ntheme\n  accent.mix = AccentColor\n    when ink=plain: #000000\n' }), 120000);
}

const SPELLINGS_SOURCE = `component App
  state fill = "#1e1e1f"
  state stroke = "1px #0000ff"
  state smooth = "antialiased"
  state size = "80px"
  state mode = "set"
  action set
    fill = "#1e1e1f"
    stroke = "1px #0000ff"
    smooth = "antialiased"
    size = "80px"
    mode = "set"
  action clear
    fill = ""
    stroke = ""
    smooth = ""
    size = ""
    mode = "empty"
  action spaces
    fill = " \\t"
    stroke = " \\t"
    smooth = " \\t"
    size = " \\t"
    mode = "spaces"
  view
    column background-color=fill
      text mode testId="mode"
      button "Set" press=set testId="set"
      button "Clear" press=clear testId="clear"
      button "Spaces" press=spaces testId="spaces"
      text "Stroke" -webkit-text-stroke=stroke testId="stroke"
      text "Smooth" -webkit-font-smoothing=smooth testId="smooth"
      button appearance="auto" width=size testId="sized"
        text "Sized"
`;

// A row whose value writes more than one property, a native button's said
// size, and the root's background as the page's canvas, in each engine.
for (const [runtime, engine] of [['JS', 'chromium'], ['wasm', 'chromium'], ['JS', 'firefox']]) {
  const check = need('fixture', runtime === 'wasm' && !process.env.EXACT_WEB_DIST ? 'name a wasm web build with EXACT_WEB_DIST' : null);
  check(`${runtime} in ${engine}: a clear unsets every property its value writes, and keeps the row said`, () => fixture(runtime, async evaluate => {
    await evaluate(`(() => {
      document.head.appendChild(document.createElement('style')).textContent =
        '#exact-root .authored-stroke{-webkit-text-stroke:2px red}#exact-root .authored-smooth{-webkit-font-smoothing:antialiased;-moz-osx-font-smoothing:grayscale}';
      document.querySelector('[data-testid="stroke"]').classList.add('authored-stroke');
      document.querySelector('[data-testid="smooth"]').classList.add('authored-smooth');
    })()`);
    const read = `(() => {
      const at = id => document.querySelector('[data-testid="' + id + '"]'), html = document.documentElement;
      const smooth = getComputedStyle(at('smooth'));
      return {
        stroke: getComputedStyle(at('stroke')).getPropertyValue('-webkit-text-stroke-width'),
        smooth: [smooth.getPropertyValue('-webkit-font-smoothing'), smooth.getPropertyValue('-moz-osx-font-smoothing')],
        said: at('sized').matches('[data-exact-said~="width"], [data-exact-width]'),
        canvas: [html.style.getPropertyValue('--exact-root-background'), getComputedStyle(html).backgroundColor],
      };
    })()`;
    for (const [id, mode] of [[null, 'set'], ['clear', 'empty'], ['set', 'set'], ['spaces', 'spaces']]) {
      if (id) await action(evaluate, id, mode);
      const actual = await evaluate(read), set = mode === 'set';
      expect(actual.stroke, mode).toBe(set ? '1px' : '0px');
      // Firefox reads WebKit's spelling as its own.
      expect(actual.smooth, mode).toEqual(engine === 'firefox' ? Array(2).fill(set ? 'grayscale' : 'auto') : [set ? 'antialiased' : 'auto', '']);
      expect(actual.said, mode).toBe(true);
      // The JS target paints the root's opaque background on the canvas; a clear leaves the shell's.
      expect(actual.canvas[0] !== '', mode).toBe(runtime === 'JS' && set);
      expect(actual.canvas[1], mode).not.toBe('rgba(0, 0, 0, 0)');
      if (runtime === 'JS' && set) expect(actual.canvas[1]).toBe('rgb(30, 30, 31)');
    }
  }, SPELLINGS_SOURCE, {}, engine), 120000);
}
