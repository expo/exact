// Build contract and exact-web-js before this suite (docs/testing.md).
import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

const root = resolve(import.meta.dir, '../..');
const target = resolve(root, 'target');
const corpus = resolve(root, 'contract/corpus/derive-form');
mkdirSync(target, { recursive: true });
const command = (bin, args) => {
  const result = spawnSync(bin, args, { cwd: root, encoding: 'utf8' });
  expect(result.status, result.stderr || result.stdout).toBe(0);
  return result;
};

async function pageFor(source, check) {
  const dir = mkdtempSync(resolve(target, 'shared-derive-'));
  let server, browser;
  try {
    writeFileSync(resolve(dir, 'app.contract'), source);
    command(resolve(target, 'debug/contract'), ['build', resolve(dir, 'app.contract'), '-o', resolve(dir, 'app.plan')]);
    command(resolve(target, 'debug/exact-web-js'), ['js', resolve(dir, 'app.plan'), '-o', dir]);
    const js = readFileSync(resolve(dir, 'app.js'), 'utf8');
    server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) return new Response('<link rel="stylesheet" href="/app.css"><div id="exact-root"></div><script type="module">import app from "/app.js";app();</script>', { headers: { 'content-type': 'text/html' } });
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(server.url.href);
    await check(page, js);
    expect(errors).toEqual([]);
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}

test('a row binding follows fields read inside a shared derive', async () => {
  await pageFor(`shape Item
  id: number
  name: string
  n: number
component App
  state items = [Item(id=1, name="one", n=1)]
  action change
    items = [Item(id=1, name="one", n=7)]
  view
    column
      button "change" press=change testId="change"
      for item in items key=item.id
        Row(item=item)
component Row
  props
    item: Item
  derive next = item.n + 1
  view
    text \`${'${item.name} ${next}'}\` testId="label"
`, async page => {
    expect(await page.getByTestId('label').textContent()).toBe('one 2');
    await page.getByTestId('change').click();
    expect(await page.getByTestId('label').textContent()).toBe('one 8');
  });
}, 60000);

test('a metric keyword behind transitive shared rows writes CSS on each branch', async () => {
  await pageFor(`component App
  state wide = true
  action change
    wide = !wide
  view
    column
      button "change" press=change testId="change"
      text "reference" width="-exact-accessory-checkmark-width" testId="reference"
      Child(wide=wide)
component Child
  props
    wide: bool
  derive metric = wide ? "-exact-accessory-checkmark-width" : "10px"
  derive width = metric
  view
    text "value" width=width testId="label"
`, async page => {
    const css = () => page.getByTestId('label').evaluate(n => n.style.width);
    const reference = await page.getByTestId('reference').evaluate(n => getComputedStyle(n).width);
    const computed = () => page.getByTestId('label').evaluate(n => getComputedStyle(n).width);
    expect(await css()).toMatch(/^[\d.]+rem$/);
    expect(await computed()).toBe(reference);
    await page.getByTestId('change').click();
    expect(await css()).toBe('10px');
    await page.getByTestId('change').click();
    expect(await css()).toMatch(/^[\d.]+rem$/);
    expect(await computed()).toBe(reference);
  });
}, 60000);

for (const stateful of [false, true]) {
  test(`derives in ${stateful ? 'slot-owning' : 'stateless'} arms keep their owner through deeper frames on JS`, async () => {
    let source = readFileSync(resolve(corpus, 'stateless.contract'), 'utf8');
    if (stateful) source = source.replace('  derive label', '  state suffix = "!"\n  derive label').replace('value + "!"', 'value + suffix');
    await pageFor(source, async (page, js) => {
      // The wrapper belongs to the owning arm, before its deeper when arm.
      expect(js).toMatch(/p=>\{(?:const \$a\d+=\{\};)?const d_\d+=\(\$budget/);
      expect(await page.locator('#exact-root').textContent()).toContain('first!');
      expect(await page.locator('#exact-root').textContent()).toContain('a!');
      await page.getByTestId('change').click();
      expect(await page.locator('#exact-root').textContent()).toContain('second!');
      expect(await page.locator('#exact-root').textContent()).toContain('c!');
      expect(await page.locator('#exact-root').textContent()).not.toContain('a!');
    });
  }, 60000);
}

test('the form keeps its encoded plan and bundled JS margins and follows real edits', async () => {
  const sizes = [], observations = [];
  for (const name of ['components', 'root']) {
    const source = resolve(corpus, `${name}.contract`);
    const dir = mkdtempSync(resolve(target, 'derive-form-bundle-'));
    try {
      command(process.execPath, ['host/web-js/build.mjs', 'hello', '--contract', source, '--out', dir, '--render', 'none']);
      sizes.push([readFileSync(resolve(dir, 'app.plan')).length, readFileSync(resolve(dir, 'app.js')).length]);
    } finally { rmSync(dir, { recursive: true, force: true }); }
    const seen = [];
    await pageFor(readFileSync(source, 'utf8'), async page => {
      const tree = () => page.locator('#exact-root').textContent();
      seen.push(await tree());
      let seed = 0x63d5n;
      for (let step = 0; step < 100; step++) {
        seed = BigInt.asUintN(64, seed * 6364136223846793005n + 1n);
        switch (step % 5) {
          case 0: await page.getByTestId('field').fill(`label ${step}`); break;
          case 1: await page.getByTestId('field').fill('x'); break;
          case 2: await page.getByTestId('toggle').click(); break;
          case 3: await page.getByTestId('rekey').click(); break;
          case 4: await page.getByTestId(`edit${seed % 19n}`).fill(`draft ${step}`); break;
        }
        seen.push(await tree());
        if (step === 0) expect(seen.at(-1)).toContain('label 0');
        if (step === 1) expect(seen.at(-1)).not.toContain('label 0');
      }
    });
    observations.push(seen);
  }
  console.log('derive form sizes (plan, bundled JS):', sizes);
  for (let i = 0; i < 2; i++) expect(sizes[0][i]).toBeLessThanOrEqual(sizes[1][i] * 1.1);
  expect(observations[0]).toEqual(observations[1]);
}, 120000);

test('65 equivalent sibling derives all render on JS', async () => {
  await pageFor(readFileSync(resolve(corpus, 'siblings.contract'), 'utf8'), async page => {
    for (let i = 0; i < 65; i++) expect(await page.getByTestId(`alias${i}`).textContent()).toBe('1');
  });
}, 60000);

test('65 compiled equivalent aliases keep their authored style', async () => {
  await pageFor(readFileSync(resolve(corpus, 'aliases.contract'), 'utf8'), async page => {
    expect(await page.getByTestId('alias64').evaluate(n => getComputedStyle(n).opacity)).toBe('0.25');
  });
}, 60000);

test('65 distinct compiled derive dependencies render their final value', async () => {
  const source = readFileSync(resolve(corpus, 'depth.contract'), 'utf8');
  for (const allReads of [false, true]) {
    const reads = '    column\n' + Array.from({ length: 65 }, (_, i) => `      text \`${'${d' + i + '}'}\` testId="d${i}"`).join('\n');
    await pageFor(allReads ? source.replace('    text `${d64}` testId="last"', reads) : source, async page => {
      expect(await page.getByTestId(allReads ? 'd64' : 'last').textContent()).toBe('65');
    });
  }
}, 60000);

test('a compiled row call limit names its LoadDerive pc', async () => {
  await pageFor(readFileSync(resolve(corpus, 'call-limit.contract'), 'utf8'), async page => {
    const logs = await page.evaluate(async () => [...(await import('/rt.js')).journal]);
    const traps = logs.filter(line => line.includes(' trapped ')).map(line => line.replace(/^t=\S+ /, ''));
    expect(traps).toEqual(['site 0: text trapped Trap(CallLimit { pc: 19 }): past the 65,536 derive calls one evaluation may take; unset']);
    expect(await page.getByTestId('last').textContent()).toBe('');
  });
}, 60000);
