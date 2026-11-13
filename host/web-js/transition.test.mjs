import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { webCompiler } from './module.mjs';

for (const target of ['js', 'wasm']) (target === 'wasm' && !process.env.EXACT_MOTION_DIST ? test.skip : test)(`${target}: declaration changes retain covered transitions and cancel uncovered eased and spring playback`, async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  const dir = mkdtempSync(resolve(root, 'target/transition-coverage-'));
  let server, browser;
  try {
    const appId = target === 'wasm' ? JSON.parse(readFileSync(resolve(process.env.EXACT_MOTION_DIST, 'bake.json'), 'utf8')).compat.inputs.app : 'test.transition-coverage';
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Transition coverage', app: { id: appId, name: 'Transition coverage' } }));
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state changed = false
  state declaration = false
  state next = "none"
  action change
    changed = true
  action cancel
    next = "none"
    declaration = true
  action other
    next = "scale 1s"
    declaration = true
  action otherSpring
    next = "scale -exact-spring()"
    declaration = true
  action sameSpring
    next = "opacity -exact-spring()"
    declaration = true
  action unset
    next = "unset"
    declaration = true
  action zero
    next = "all 0s"
    declaration = true
  view
    column
      button "Change" press=change testId="change"
      button "None" press=cancel testId="none"
      button "Other" press=other testId="other"
      button "Other spring" press=otherSpring testId="otherSpring"
      button "Same spring" press=sameSpring testId="sameSpring"
      button "Unset" press=unset testId="unset"
      button "Zero" press=zero testId="zero"
      box testId="ease" opacity=(changed ? 0.2 : 1) transition=(declaration ? next : "opacity 2s linear")
      box testId="spring" opacity=(changed ? 0.2 : 1) transition=(declaration ? next : "opacity -exact-spring()")
`);
    const out = resolve(dir, 'dist');
    if (target === 'js') {
      const built = spawnSync(process.execPath, ['host/web-js/build.mjs', dir, '--out', out, '--render', 'none'], { cwd: root, encoding: 'utf8' });
      expect(built.status, built.stdout + built.stderr).toBe(0);
    } else {
      const plan = resolve(dir, 'app.plan');
      const built = spawnSync(resolve(root, 'target/debug/contract'), ['build', resolve(dir, 'app.contract'), '-o', plan], { cwd: root, encoding: 'utf8' });
      expect(built.status, built.stdout + built.stderr).toBe(0);
      cpSync(process.env.EXACT_MOTION_DIST, out, { recursive: true });
      function bootPlan(plan) {
        const instantiate = WebAssembly.instantiateStreaming;
        WebAssembly.instantiateStreaming = async (...args) => {
          const result = await instantiate(...args), w = result.instance.exports;
          let input;
          return { ...result, instance: { exports: { ...w,
            exact_in(n) { input = w.exact_in(n); return input; },
            exact_boot(width, height, n, preferences) {
              const launch = new Uint8Array(w.memory.buffer, input, n).slice(), ptr = w.exact_in(plan.length + n);
              new Uint8Array(w.memory.buffer, ptr, plan.length).set(plan);
              new Uint8Array(w.memory.buffer, ptr + plan.length, n).set(launch);
              return w.exact_boot_plan(plan.length, width, height, n, preferences);
            },
          } } };
        };
      }
      const script = '<script type="module" src="./glue.js"></script>';
      const html = readFileSync(resolve(out, 'index.html'), 'utf8');
      expect(html).toContain(script);
      writeFileSync(resolve(out, 'index.html'), html.replace(script, `<script>(${bootPlan})(${JSON.stringify([...readFileSync(plan)])})</script>${script}`));
    }
    server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch: request => new Response(Bun.file(resolve(out, new URL(request.url).pathname.slice(1) || 'index.html'))) });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    for (const next of ['none', 'other', 'otherSpring', 'sameSpring', 'unset', 'zero']) {
      await page.goto(server.url.href);
      await page.waitForFunction(() => document.querySelector('[data-testid="spring"]') && (typeof globalThis.exact?.settleAt === 'function' || globalThis.exact?.root?.dataset.moduleReady === 'true')).catch(error => { throw new Error(`${target}: ${errors.join('\n')}`, { cause: error }); });
      const rows = await page.evaluate(async next => {
        const elements = ['ease', 'spring'].map(id => document.querySelector(`[data-testid="${id}"]`));
        elements.forEach(el => getComputedStyle(el).opacity);
        document.querySelector('[data-testid="change"]').click();
        await Promise.resolve();
        const animations = elements.map(el => el.getAnimations()[0]);
        animations.forEach(a => { a.pause(); a.currentTime = 100; });
        const before = elements.map(el => Number(getComputedStyle(el).opacity));
        document.querySelector(`[data-testid="${next}"]`).click();
        await Promise.resolve();
        return elements.map((el, i) => ({ before: before[i], after: Number(getComputedStyle(el).opacity), retained: el.getAnimations().includes(animations[i]), state: animations[i].playState }));
      }, next);
      for (const row of rows) {
        expect(row.before).toBeGreaterThan(0.2);
        expect(row.before).toBeLessThan(1);
        expect(row.retained, `${next}: ${JSON.stringify(rows)}`).toBe(['sameSpring', 'unset', 'zero'].includes(next));
        if (row.retained) expect(row.after, `${target}/${next}: ${JSON.stringify(rows)}`).toBeCloseTo(row.before);
        else { expect(row.state).toBe('idle'); expect(row.after).toBeCloseTo(0.2); }
      }
    }
    expect(errors).toEqual([]);
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

test('bound springs animate paint and type on ordinary and pressed rows', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/spring-transition-'));
  let server, browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Spring transition', app: { id: 'test.spring-transition', name: 'Spring transition' } }));
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state changed = false
  state curve = "all -exact-spring() 0s 0.1s, font-weight 2s linear 0s"
  state typeCurve = "font-weight 2s linear, font-weight -exact-spring(100, 20, 1) 0s 125ms"
  action change
    changed = not changed
  view
    column
      button "Change" press=change testId="change"
      text "Ordinary" testId="ordinary" color=(changed ? "#ffffff" : "#000000") font-weight=(changed ? 800 : 200) transition=curve
      text "Pressed" testId="pressed" press=change -exact-press-scale=0.9 color=(changed ? "#ffffff" : "#000000") font-weight=(changed ? 800 : 200) transition=curve
      text "Spring type" testId="type" font-weight=(changed ? 800 : 200) transition=typeCurve
`);
    const out = resolve(dir, 'dist');
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', dir, '--out', out, '--render', 'none'], { cwd: root, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch: request => new Response(Bun.file(resolve(out, new URL(request.url).pathname.slice(1) || 'index.html'))) });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => document.querySelector('[data-testid="ordinary"]'));
    const observed = await page.evaluate(() => {
      const elements = ['ordinary', 'pressed'].map(id => document.querySelector(`[data-testid="${id}"]`));
      elements.forEach(el => getComputedStyle(el).color);
      document.querySelector('[data-testid="change"]').click();
      const paint = elements.map(el => {
        const animations = el.getAnimations();
        const color = animations.find(a => a.transitionProperty === 'color');
        const weight = animations.find(a => a.transitionProperty === 'font-weight');
        const result = { properties: animations.map(a => a.transitionProperty).sort(), running: color?.playState, easing: color?.effect.getTiming().easing, delay: color?.effect.getTiming().delay, weightDuration: weight?.effect.getTiming().duration };
        for (const a of animations) { a.pause(); a.currentTime = a.transitionProperty === 'color' ? 200 : 1000; }
        result.color = getComputedStyle(el).color;
        result.weight = getComputedStyle(el).fontWeight;
        animations.forEach(a => a.finish());
        result.finalColor = getComputedStyle(el).color;
        return result;
      });
      const type = document.querySelector('[data-testid="type"]');
      const animation = type.getAnimations().find(a => a.transitionProperty === 'font-weight');
      const typography = { running: animation?.playState, easing: animation?.effect.getTiming().easing, delay: animation?.effect.getTiming().delay };
      if (animation) { animation.pause(); animation.currentTime = 300; }
      typography.weight = Number(getComputedStyle(type).fontWeight);
      animation?.finish();
      typography.finalWeight = Number(getComputedStyle(type).fontWeight);
      return { paint, typography };
    });
    for (const value of observed.paint) {
      expect(value.properties).toEqual(['color', 'font-weight']);
      expect(value.running).toBe('running');
      expect(value.easing).toStartWith('linear(');
      expect(value.delay).toBeCloseTo(100);
      expect(value.color).not.toBe('rgb(0, 0, 0)');
      expect(value.color).not.toBe('rgb(255, 255, 255)');
      expect(value.finalColor).toBe('rgb(255, 255, 255)');
      expect(value.weightDuration).toBe(2000);
      expect(Number(value.weight)).toBeCloseTo(500);
    }
    expect(observed.typography.running).toBe('running');
    expect(observed.typography.easing).toStartWith('linear(');
    expect(observed.typography.delay).toBeCloseTo(125);
    expect(observed.typography.weight).toBeGreaterThan(200);
    expect(observed.typography.weight).toBeLessThan(800);
    expect(observed.typography.finalWeight).toBe(800);
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

test('authored per-axis velocity reaches the web spring without changing its endpoint', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/spring-velocity-'));
  let server, browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Spring velocity', app: { id: 'test.spring-velocity', name: 'Spring velocity' } }));
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state target = 0
  state velocity = "800 -600"
  state spring = false
  action release
    spring = true
    target = 100
  action velocityOnly
    velocity = "999 999"
  view
    column
      button "Release" press=release testId="release"
      button "Velocity only" press=velocityOnly testId="velocity-only"
      box testId="forward" width=100 height=100 translate=\`\${target}px \${target}px\` -exact-translate-velocity=velocity transition="translate -exact-spring(260, 24, 1)"
      box testId="reverse" width=100 height=100 translate=\`\${target}px \${target}px\` -exact-translate-velocity="-800 600" transition="translate -exact-spring(260, 24, 1)"
      box testId="zero" width=100 height=100 translate=\`\${target}px \${target}px\` -exact-translate-velocity="0 0" transition="translate -exact-spring(260, 24, 1)"
      box testId="same-endpoint" width=100 height=100 translate="0px 0px" -exact-translate-velocity="800 -600" transition=(spring ? "translate -exact-spring(260, 24, 1)" : "none")
`);
    const out = resolve(dir, 'dist');
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', dir, '--out', out, '--render', 'none'], { cwd: root, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch: request => new Response(Bun.file(resolve(out, new URL(request.url).pathname.slice(1) || 'index.html'))) });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => typeof globalThis.exact?.settleAt === 'function');
    const shown = await page.evaluate(async () => {
      const boxes = ['forward', 'reverse', 'zero', 'same-endpoint'].map(id => document.querySelector(`[data-testid="${id}"]`));
      document.querySelector('[data-testid="release"]').click();
      await Promise.resolve();
      const animations = boxes.map(el => el.getAnimations().find(a => a.effect.getKeyframes().some(k => k.translate)));
      for (const a of animations) { a.pause(); a.currentTime = 1; }
      const starts = boxes.map(el => getComputedStyle(el).translate.split(' ').map(parseFloat));
      document.querySelector('[data-testid="velocity-only"]').click();
      await Promise.resolve();
      const same = boxes[0].getAnimations().includes(animations[0]);
      animations.forEach(a => a.finish());
      return { starts, same, ends: boxes.map(el => { const [x, y = 0] = getComputedStyle(el).translate.split(' ').map(parseFloat); return [x, y]; }) };
    });
    expect(shown.starts[0][0]).toBeGreaterThan(0);
    expect(shown.starts[0][1]).toBeLessThan(0);
    expect(shown.starts[1][0]).toBeLessThan(0);
    expect(shown.starts[1][1]).toBeGreaterThan(0);
    expect(shown.starts[2][0]).toBeGreaterThan(0);
    expect(shown.starts[2][0]).toBeCloseTo(shown.starts[2][1]);
    expect(shown.starts[3][0]).toBeGreaterThan(0);
    expect(shown.starts[3][1]).toBeLessThan(0);
    expect(shown.same).toBe(true);
    expect(shown.ends).toEqual([[100, 100], [100, 100], [100, 100], [0, 0]]);
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);
