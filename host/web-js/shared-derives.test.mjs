// Build contract and exact-web-js before this suite (docs/testing.md).
import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { builtFile } from './content-names.mjs';

const root = resolve(import.meta.dir, '../..');
const target = resolve(root, 'target');
const corpus = resolve(root, 'contract/corpus/derive-form');
mkdirSync(target, { recursive: true });
const command = (bin, args) => {
  const result = spawnSync(bin, args, { cwd: root, encoding: 'utf8' });
  expect(result.status, result.stderr || result.stdout).toBe(0);
  return result;
};

async function pageFor(source, check, flags = []) {
  const dir = mkdtempSync(resolve(target, 'shared-derive-'));
  let server, browser;
  try {
    writeFileSync(resolve(dir, 'app.contract'), source);
    command(resolve(target, 'debug/contract'), ['build', resolve(dir, 'app.contract'), '-o', resolve(dir, 'app.plan')]);
    command(resolve(target, 'debug/exact-web-js'), ['js', resolve(dir, 'app.plan'), '-o', dir, ...flags]);
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

test('two bindings share a conditional program and follow its field in each keyed row', async () => {
  await pageFor(`shape Item
  id: number
  level: number
fn label(n: number): string = n > 1 ? "Large frame" : "Small frame"
component App
  state items = [Item(id=1, level=1), Item(id=2, level=2)]
  action change
    items = [Item(id=2, level=1), Item(id=1, level=2)]
  view
    column
      button "Change" press=change testId="change"
      for item in items key=item.id
        text label(item.level) aria-label=label(item.level) testId=item.id.toString()
`, async (page, js) => {
    expect(js.split('"Large frame"')).toHaveLength(2);
    const values = id => page.getByTestId(id).evaluate(e => [e.textContent, e.getAttribute('aria-label')]);
    expect(await values('1')).toEqual(['Small frame', 'Small frame']);
    expect(await values('2')).toEqual(['Large frame', 'Large frame']);
    await page.getByTestId('change').click();
    expect(await values('1')).toEqual(['Large frame', 'Large frame']);
    expect(await values('2')).toEqual(['Small frame', 'Small frame']);
  });
}, 60000);

test('two regions share their condition and update the right keyed row', async () => {
  await pageFor(`shape Item
  id: string
  detail: string
fn shows(item: Item): bool = item.detail == "expanded" ? item.id != "archived" : false
component App
  state items = [Item(id="one", detail="compact"), Item(id="two", detail="expanded")]
  action change
    items = [Item(id="two", detail="compact"), Item(id="one", detail="expanded")]
  view
    column
      button "Change" press=change testId="change"
      for item in items key=item.id
        column testId=item.id
          if shows(item)
            text "Title"
          if shows(item)
            link href="/details"
              text "Details"
`, async (page, js) => {
    expect(js.split('"archived"')).toHaveLength(2);
    const content = id => page.getByTestId(id).textContent();
    expect(await content('one')).toBe('');
    expect(await content('two')).toBe('TitleDetails');
    await page.getByTestId('change').click();
    expect(await content('one')).toBe('TitleDetails');
    expect(await content('two')).toBe('');
  });
}, 60000);

test('a symbol uses its current colour through disabled state changes', async () => {
  await pageFor(`component App
  state off = false
  action go
    off = not off
  view
    column color="#0000ff"
      button "Toggle" press=go testId="toggle"
      button press=go appearance="none"
        image "symbol:star" testId="stated" disabled=off -exact-tint-color="#ff0000"
          :disabled
            -exact-tint-color="#00ff00"
`, async page => {
    const image = page.getByTestId('stated');
    const ink = () => image.evaluate(e => [e.hasAttribute('disabled'), e.style.getPropertyValue('--exact-tint').trim().toLowerCase()]);
    expect(await ink()).toEqual([false, 'currentcolor']);
    await page.getByTestId('toggle').click();
    expect(await ink()).toEqual([true, 'currentcolor']);
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
      expect(js).toMatch(/(?:p|\(p\))=>\{(?:const \$a\d+=\{\};)?const d_L\d+_\d+=\$atRead\(\$budget=>/);
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
      sizes.push([readFileSync(resolve(dir, 'app.plan')).length, readFileSync(resolve(dir, builtFile(dir, 'app.js'))).length]);
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

test('nested slot arms share mount code and keep each row and arm instance', async () => {
  const layers = 6;
  const source = [
    'shape DetailData',
    '  title: string',
    'component App',
    '  state raised = false',
    '  state picked = 0',
    '  state broken = false',
    '  state detail = false',
    '  action flip',
    '    raised = !raised',
    '  action overflow',
    '    broken = true',
    '  action pick(n: number)',
    '    picked = n',
    '  view',
    '    column',
    '      button "Toggle" press=flip testId="toggle"',
    '      button "Overflow" press=overflow testId="overflow"',
    '      text `${picked}` testId="picked"',
    '      if detail',
    '        Detail()',
    '      for n in [4, 9] key=n',
    ...Array.from({ length: layers }, (_, i) => ' '.repeat(8 + 2 * i) + `Layer${i}(raised=raised)`),
    ' '.repeat(8 + 2 * layers) + 'button "Only visible leaf" aria-label="row\\u0000s0\\u0000" testId=`leaf-${n}` press=pick(range(broken ? 65537 : n).length)',
    ...Array.from({ length: layers }, (_, i) => `component Layer${i}
  props
    raised: bool
  slot
  view
    if raised
      column padding=8 testId="layer-${i}"
        children
    else
      column padding=4 testId="layer-${i}"
        children`),
    'component Detail',
    '  resource data = readDetail() as shape DetailData else empty()',
    '  view',
    '    text data.title',
  ].join('\n') + '\n';
  const reports = [];
  for (const sites of [false, true]) {
    const traps = [];
    await pageFor(source, async (page, js) => {
      expect(js.split('Only visible leaf').length - 1).toBeLessThanOrEqual(2);
      for (let i = 0; i < layers; i++) expect(js.split(`"layer-${i}"`).length - 1).toBeLessThanOrEqual(2);
      const leaf = page.getByTestId('leaf-9');
      expect(await leaf.getAttribute('aria-label')).toBe('row\0s0\0');
      const initial = await leaf.elementHandle();
      for (const [id, padding] of [[9, 4], [4, 8], [9, 4]]) {
        await page.getByTestId(`leaf-${id}`).click();
        expect(await page.getByTestId('picked').textContent()).toBe(String(id));
        expect(await leaf.evaluate((e, depth) => {
          const values = [];
          for (let i = 0; i < depth; i++) {
            e = e.parentElement;
            values.push(getComputedStyle(e).paddingTop);
          }
          return values;
        }, layers)).toEqual(Array(layers).fill(`${padding}px`));
        await page.getByTestId('toggle').click();
        expect(await initial.evaluate(e => e.isConnected)).toBe(false);
      }
      await page.getByTestId('overflow').click();
      for (let branch = 0; branch < 2; branch++) {
        await leaf.click();
        const report = await page.evaluate(async () =>
          (await import('/rt.js')).journal.filter(line => line.includes('press argument 1')).at(-1));
        expect(report).toContain('IterationLimit');
        const site = Number(/site (\d+) press/.exec(report)[1]);
        if (sites) expect(site).toBe(Number(await leaf.getAttribute('data-site')));
        traps.push(site);
        await page.getByTestId('toggle').click();
      }
    }, sites ? ['--sites'] : []);
    expect(traps[0]).not.toBe(traps[1]);
    reports.push(traps);
  }
  expect(reports[0]).toEqual(reports[1]);
}, 60000);

test('a shared slot frame reads the getter declared for its card in each keyed row', async () => {
  const source = `component App
  state limit = 2
  action overflow
    limit = 65537
  view
    column
      button "Overflow" press=overflow testId="overflow"
      for n in [1, 4] key=n
        column
          Meter(count=range(limit + n).length)
            text "A"
          Meter(count=range(limit + n + 1).length)
            text "B"
            text n.toString()
          Meter(count=range(limit + n + 2).length)
            if n == 1
              text "C"
component Meter
  props
    count: number
  slot
  view
    column aria-label="Row meter"
      text \`(\${count})\`
      children
      for suffix in ["!"] key=suffix
        Suffix(value=count + 1, suffix=suffix)
component Suffix
  props
    value: number
    suffix: string
  view
    text \`[\${value}]\${suffix}\`
`;
  const reports = [];
  for (const sites of [false, true]) {
    await pageFor(source, async (page, js) => {
      expect(js.split('"Row meter"')).toHaveLength(2);
      expect(await page.getByLabel('Row meter').allTextContents()).toEqual([
        '(3)A[4]!', '(4)B1[5]!', '(5)C[6]!', '(6)A[7]!', '(7)B4[8]!', '(8)[9]!',
      ]);
      await page.getByTestId('overflow').click();
      const trapped = await page.evaluate(async () => (await import('/rt.js')).journal
        .filter(line => line.includes('IterationLimit')).map(line => line.replace(/^t=\S+ /, '')).sort());
      expect(trapped).toHaveLength(12);
      reports.push(trapped);
    }, sites ? ['--sites'] : []);
  }
  expect(reports[0]).toEqual(reports[1]);
}, 60000);

test('a shared list reads state from the panel that mounted it', async () => {
  await pageFor(`component App
  state shown = true
  state items = [1, 2]
  action toggle
    shown = !shown
  view
    column
      button "Toggle" press=toggle testId="toggle"
      if shown
        Panel(id="left", numbers=items)
      if shown
        Panel(id="right", numbers=items)
          text "Extra" testId="extra"

component Panel
  props
    id: string
    numbers: number[]
  slot
  state count = 0
  derive label = id + " row"
  action advance
    count = count + 1
  view
    column testId=id
      for n in numbers key=n
        button press=advance aria-label="Increase row count"
          text \`\${label} \${n}: \${count}\`
      if count >= 0
        children
`, async (page, js) => {
    expect(js.split('"Increase row count"')).toHaveLength(2);
    const rows = id => page.getByTestId(id).getByRole('button', { name: 'Increase row count' });
    await rows('left').nth(1).click();
    expect(await rows('left').allTextContents()).toEqual(['left row 1: 1', 'left row 2: 1']);
    expect(await rows('right').allTextContents()).toEqual(['right row 1: 0', 'right row 2: 0']);
    expect(await page.getByTestId('right').getByTestId('extra').count()).toBe(1);
    await rows('right').first().click();
    expect(await rows('right').allTextContents()).toEqual(['right row 1: 1', 'right row 2: 1']);
    await page.getByTestId('toggle').click();
    expect(await rows('left').count()).toBe(0);
    await page.getByTestId('toggle').click();
    expect(await rows('left').allTextContents()).toEqual(['left row 1: 0', 'left row 2: 0']);
  });
}, 60000);

test.each([true, false])('repeated components bind their own props, actions and state (conditional %s)', async conditional => {
  const source = `component App
  state left = "Left"
  state right = "Right"
  state who = ""
  state shown = true
  action pickLeft
    who = left
  action pickRight
    who = right
  action edit
    right = right + "!"
  action toggle
    shown = !shown
  view
    column
      Card(title=left, id="left", description="left", pick=pickLeft, shown=shown)
      Card(title=right, id="right", description="right-description", pick=pickRight, shown=shown)
      button "Edit" press=edit testId="edit"
      button "Toggle" press=toggle testId="toggle"
      text who testId="who"
component Card
  props
    title: string
    id: string
    description: string
    pick: action
    shown: bool
  view
    ${conditional ? 'if shown\n      ' : ''}Counter(title=title, id=id, description=description, pick=pick)
component Counter
  props
    title: string
    id: string
    description: string
    pick: action
  state count = 0
  derive label = title + " count"
  action advance
    count = count + 1
    pick()
  view
    button press=advance testId=id aria-label="Shared counter button" aria-describedby=description
      text \`\${label} \${count}\`
`;
  await pageFor(source, async (page, js) => {
    expect(js.split('"Shared counter button"').length - 1).toBe(1);
    const left = page.getByTestId('left'), right = page.getByTestId('right');
    expect(await left.getAttribute('aria-describedby')).toBe('left');
    expect(await right.getAttribute('aria-describedby')).toBe('right-description');
    expect(await left.textContent()).toBe('Left count 0');
    expect(await right.textContent()).toBe('Right count 0');
    await right.click();
    expect(await page.getByTestId('who').textContent()).toBe('Right');
    expect(await left.textContent()).toBe('Left count 0');
    expect(await right.textContent()).toBe('Right count 1');
    await page.getByTestId('edit').click();
    expect(await right.textContent()).toBe('Right! count 1');
    await left.click();
    expect(await page.getByTestId('who').textContent()).toBe('Left');
    await page.getByTestId('toggle').click();
    expect(await right.count()).toBe(conditional ? 0 : 1);
    await page.getByTestId('toggle').click();
    expect(await left.textContent()).toBe(conditional ? 'Left count 0' : 'Left count 1');
    expect(await right.textContent()).toBe(conditional ? 'Right! count 0' : 'Right! count 1');
  });
}, 60000);

test('one card mount reads direct and computed props and reports traps at their own sites', async () => {
  const source = `component App
  state n = 2
  state m = 3
  derive settled = n + 4
  action bump
    n = n + 1
  action overflow
    n = 65537
  view
    column
      Card(count=n, id="direct")
      Card(count=settled, id="settled")
      Card(count=n * m + 1, id="product")
      Card(count=(m > 0 ? n + m : 0), id="branch")
      Card(count=range(n).length, id="steps")
      button "Bump" press=bump testId="bump"
      button "Overflow" press=overflow testId="overflow"
component Card
  props
    count: number
    id: string
  view
    column testId=id aria-label="Shared numeric card"
      text \`total \${count}\`
      text \`again \${count}\`
`;
  const traps = [];
  for (const sites of [false, true]) {
    await pageFor(source, async (page, js) => {
      expect(js.split('"Shared numeric card"').length - 1).toBe(1);
      const cards = page.getByLabel('Shared numeric card');
      const totals = ns => ns.map(n => `total ${n}again ${n}`);
      expect(await cards.allTextContents()).toEqual(totals([2, 6, 7, 5, 2]));
      await page.getByTestId('bump').click();
      expect(await cards.allTextContents()).toEqual(totals([3, 7, 10, 6, 3]));
      await page.getByTestId('overflow').click();
      expect(await cards.allTextContents()).toEqual([...totals([65537, 65541, 196612, 65540]), '']);
      const lines = await page.evaluate(async () => (await import('/rt.js')).journal
        .filter(line => line.includes('text trapped Trap(IterationLimit'))
        .map(line => line.replace(/^t=\S+ /, '')));
      expect(lines).toHaveLength(2);
      traps.push(lines);
    }, sites ? ['--sites'] : []);
  }
  expect(traps[0]).toEqual(traps[1]);
}, 60000);

test('shared action code writes its own slot and reports the name of the failing action', async () => {
  const source = `component App
  state n = 2
  state left = ""
  state right = ""
  action writeLeft(label: string)
    left = \`\${label} results \${range(n).length}\`
  action writeRight(caption: string)
    right = \`\${caption} results \${range(n).length}\`
  action overflow
    n = 65537
  view
    column
      button "Left" testId="first" press=writeLeft("First")
      button "Right" testId="second" press=writeRight("Second")
      button "Overflow" testId="overflow" press=overflow
      text left testId="left"
      text right testId="right"
`;
  for (const sites of [false, true]) {
    await pageFor(source, async (page, js) => {
      expect(js.split('" results "').length - 1).toBe(1);
      await page.getByTestId('second').click();
      expect(await page.getByTestId('right').textContent()).toBe('Second results 2');
      expect(await page.getByTestId('left').textContent()).toBe('');
      await page.getByTestId('first').click();
      expect(await page.getByTestId('left').textContent()).toBe('First results 2');
      await page.getByTestId('overflow').click();
      for (const [id, name] of [['second', 'writeRight'], ['first', 'writeLeft']]) {
        await page.getByTestId(id).click();
        const line = await page.evaluate(async () =>
          (await import('/rt.js')).journal.filter(entry => entry.includes('IterationLimit')).at(-1));
        expect(line).toContain(`action "${name}"`);
      }
    }, sites ? ['--sites'] : []);
  }
}, 60000);

test('cards with different slot contents share their frame and retain each row binding', async () => {
  await pageFor(`component App
  state active = ""
  action choose(id: string)
    active = id
  view
    column
      for id in ["Alice", "Bob"] key=id
        column
          Card()
            text id
          Card()
            text "Choose"
            button id press=choose(id)
          Card()
            if active == id
              text "Selected"
            else
              text "Waiting"
          column aria-label="Row control"
            row
              button id press=choose(id)
component Card
  slot
  view
    column aria-label="Shared card"
      text "Header"
      children
      text "Footer"
`, async (page, js) => {
    for (const fixed of ['"Shared card"', '"Header"', '"Footer"']) expect(js.split(fixed)).toHaveLength(2);
    expect(js).not.toContain('((p,$slot)=>');
    const cards = page.getByLabel('Shared card');
    expect(await cards.allTextContents()).toEqual([
      'HeaderAliceFooter', 'HeaderChooseAliceFooter', 'HeaderWaitingFooter',
      'HeaderBobFooter', 'HeaderChooseBobFooter', 'HeaderWaitingFooter',
    ]);
    await cards.nth(4).getByRole('button', { name: 'Bob' }).click();
    expect(await cards.nth(5).textContent()).toBe('HeaderSelectedFooter');
    expect(await cards.nth(2).textContent()).toBe('HeaderWaitingFooter');
    await page.getByLabel('Row control').getByRole('button', { name: 'Alice' }).click();
    expect(await cards.nth(2).textContent()).toBe('HeaderSelectedFooter');
  });
}, 60000);

test('content after a variable-length slot reports each binding at its plan site', async () => {
  const source = `component App
  state limit = 2
  action overflow
    limit = 65537
  view
    column
      button "Overflow" press=overflow testId="overflow"
      Meter(count=range(limit).length)
        text "A"
      Meter(count=range(limit).length)
        text "B"
        button "Again" press=overflow
      Meter(count=range(limit).length)
        if limit > 0
          text "C"
component Meter
  props
    count: number
  slot
  view
    column aria-label="Slot meter"
      text \`Before \${count}\`
      children
      text \`After \${count}\`
`;
  const reports = [];
  for (const sites of [false, true]) {
    await pageFor(source, async (page, js) => {
      expect(js.split('"Slot meter"')).toHaveLength(2);
      expect(await page.getByLabel('Slot meter').allTextContents()).toEqual([
        'Before 2AAfter 2', 'Before 2BAgainAfter 2', 'Before 2CAfter 2',
      ]);
      await page.getByTestId('overflow').click();
      const trapped = await page.evaluate(async () => (await import('/rt.js')).journal
        .filter(line => line.includes('IterationLimit')).map(line => line.replace(/^t=\S+ /, '')).sort());
      expect(trapped).toHaveLength(6);
      reports.push(trapped);
    }, sites ? ['--sites'] : []);
  }
  expect(reports[0]).toEqual(reports[1]);
}, 60000);

test('components with state, branches and internal loops share one mount across nesting depths', async () => {
  const source = `shape Item
  id: string
  name: string

component App
  state tab: string = "a"
  state show: bool = true
  state items: Item[] = [Item(id="1", name="One"), Item(id="2", name="Two")]
  state opts: Item[] = [Item(id="x", name="Alpha"), Item(id="y", name="Beta")]
  view
    column
      if show
        column
          for item in items key=item.id
            Dropdown(id=\`d2-\${item.id}\`, title=item.name, options=opts)
      if tab == "a"
        column
          if show
            column
              for item in items key=item.id
                Dropdown(id=\`d3-\${item.id}\`, title=item.name, options=opts)

component Dropdown
  props
    id: string
    title: string
    options: Item[]
  state open: bool = false
  state picked: string = "none"
  action toggle
    open = !open
  action pick(name: string)
    picked = name
    open = false
  view
    column aria-label="Dropdown box"
      button press=toggle testId=\`btn-\${id}\`
        text \`\${title}:\${picked}\`
      if open
        column aria-label="Dropdown choice"
          for opt in options key=opt.id
            button press=pick(opt.name) testId=\`opt-\${id}-\${opt.id}\`
              text opt.name
`;
  await pageFor(source, async (page, js) => {
    expect(js.split('"Dropdown box"')).toHaveLength(2);
    expect(js.split('"Dropdown choice"')).toHaveLength(2);
    expect(await page.getByTestId('btn-d2-1').textContent()).toBe('One:none');
    expect(await page.getByTestId('btn-d3-2').textContent()).toBe('Two:none');
    await page.getByTestId('btn-d2-1').click();
    await page.getByTestId('opt-d2-1-x').click();
    expect(await page.getByTestId('btn-d2-1').textContent()).toBe('One:Alpha');
    expect(await page.getByTestId('btn-d2-2').textContent()).toBe('Two:none');
    expect(await page.getByTestId('btn-d3-1').textContent()).toBe('One:none');
    await page.getByTestId('btn-d3-2').click();
    await page.getByTestId('opt-d3-2-y').click();
    expect(await page.getByTestId('btn-d3-2').textContent()).toBe('Two:Beta');
    expect(await page.getByTestId('btn-d2-1').textContent()).toBe('One:Alpha');
  });
}, 60000);

test('shared mounts cross match payload arms and inlined nested loops keep outer items', async () => {
  const source = `shape Group
  id: string
  name: string
  cells: string[]

component App
  state first = some("Ready")
  state second = some("Next")
  state groups: Group[] = [Group(id="a", name="A", cells=["1", "2"]), Group(id="b", name="B", cells=["3"])]
  action swap
    first = some("Done")
    second = some("After")
  view
    column
      button "Swap" press=swap testId="swap"
      match first
        case some(value)
          Badge(label=value)
        case none
          text "Empty"
      match second
        case some(value)
          Badge(label=value)
        case none
          text "Empty"
      for group in groups key=group.id
        column
          for cell in group.cells key=cell
            text \`\${group.name}:\${cell}\` testId=\`cell-\${group.id}-\${cell}\`

component Badge
  props
    label: string
  view
    box aria-label="Shared badge"
      text \`[\${label}]\`
`;
  await pageFor(source, async (page, js) => {
    expect(js.split('"Shared badge"')).toHaveLength(2);
    expect(await page.getByLabel('Shared badge').allTextContents()).toEqual(['[Ready]', '[Next]']);
    expect(await page.getByTestId('cell-a-1').textContent()).toBe('A:1');
    expect(await page.getByTestId('cell-a-2').textContent()).toBe('A:2');
    expect(await page.getByTestId('cell-b-3').textContent()).toBe('B:3');
    await page.getByTestId('swap').click();
    expect(await page.getByLabel('Shared badge').allTextContents()).toEqual(['[Done]', '[After]']);
  });
}, 60000);

test('virtualized rows keep outer item getters and nested owned state in DOM and logical text', async () => {
  const source = `component App
  state groups = range(80)
  view
    list virtualized=true height=160 estimated-item-height=40 testId="vlist"
      for group in groups key=group
        Row(group=group)

component Row
  props
    group: number
  state shown = group >= 50
  derive tag = \`R\${group}\`
  view
    column height=40 testId=\`row-\${group}\`
      for cell in ["X", "Y"] key=cell
        text \`\${group}:\${cell}:\${tag}\` testId=\`cell-\${group}-\${cell}\`
      if shown
        Detail(group=group, tag=tag)

component Detail
  props
    group: number
    tag: string
  state count = 1
  derive label = \`\${tag}-D\${group}-\${count}\`
  view
    text label testId=\`detail-\${group}\`
`;
  await pageFor(source, async (page, js) => {
    expect(await page.getByTestId('cell-0-X').textContent()).toBe('0:X:R0');
    expect(await page.getByTestId('cell-0-Y').textContent()).toBe('0:Y:R0');
    expect(await page.getByTestId('cell-1-X').textContent()).toBe('1:X:R1');
    expect(js).not.toMatch(/const (\$a\d+)=\{\.\.\.\1\}/);
    // Project an unmounted row above 50 through the compiled logical text callback.
    const unmounted = await page.evaluate(() => {
      const el = document.querySelector('[data-testid="vlist"]');
      const flatten = nodes => nodes.flatMap(([k, props, kids]) => [
        ...(k === 1 && props.text != null ? [props.text] : []),
        ...flatten(kids),
      ]);
      return flatten(el.$list.o.text(() => 60, () => 60));
    });
    expect(unmounted).toEqual(['60:X:R60', '60:Y:R60', 'R60-D60-1']);
  });
}, 60000);

test('development builds (--sites and --dev-reload) share mounts and keep per-site data-site, perf counts, and each regions', async () => {
  const source = `shape Opt
  id: string
  label: string

component App
  state leftMode = "a"
  state rightMode = "b"
  state opts: Opt[] = [Opt(id="x", label="Alpha"), Opt(id="y", label="Beta")]
  action flipRight
    rightMode = rightMode == "a" ? "b" : "a"
  view
    column
      button "Flip right" press=flipRight testId="flip-right"
      Card(id="left", mode=leftMode, opts=opts)
        text "One"
      Card(id="right", mode=rightMode, opts=opts)
        text "Two"
        text "Extra"

component Card
  props
    id: string
    mode: string
    opts: Opt[]
  slot
  state count = 0
  action bump
    count = count + 1
  view
    column testId=\`card-\${id}\` aria-label="Dev shared card"
      text \`\${id}:\${count}\` testId=\`val-\${id}\` color=(mode == "a" ? "#111111" : "#224488")
      children
      button "Bump" press=bump testId=\`bump-\${id}\`
      for opt in opts key=opt.id
        text \`\${id}-\${opt.label}\` testId=\`opt-\${id}-\${opt.id}\`
`;
  await pageFor(source, async (page, js) => {
    expect(js.split('"Dev shared card"')).toHaveLength(2);
    expect(js.split('"Bump"')).toHaveLength(2);
    expect(js.split('Pc(')).toHaveLength(2);
    expect(js.split('each(')).toHaveLength(2);

    const cardSites = async id => ({
      card: Number(await page.getByTestId(`card-${id}`).getAttribute('data-site')),
      val: Number(await page.getByTestId(`val-${id}`).getAttribute('data-site')),
      bump: Number(await page.getByTestId(`bump-${id}`).getAttribute('data-site')),
      optX: Number(await page.getByTestId(`opt-${id}-x`).getAttribute('data-site')),
      optY: Number(await page.getByTestId(`opt-${id}-y`).getAttribute('data-site')),
      carry: await page.getByTestId(`card-${id}`).getAttribute('data-carry-type'),
    });
    const left = await cardSites('left');
    const right = await cardSites('right');

    // Distinct plan sites across instances, including after the variable-length slot.
    expect(left.carry).toBe('View');
    expect(right.carry).toBe('View');
    expect(new Set([left.card, left.val, left.bump, left.optX, right.card, right.val, right.bump, right.optX]).size).toBe(8);
    expect(left.optX).toBe(left.optY);
    expect(right.optX).toBe(right.optY);
    expect(right.bump - right.card).toBe(left.bump - left.card + 1);

    // Install perf.js and verify evaluations attribute to the right site for both pf bindings and lowered Pc styles.
    await page.evaluate(async () => {
      const { install } = await import('/perf.js');
      install(document.getElementById('exact-root'));
    });
    await page.getByTestId('bump-left').click();
    await page.getByTestId('flip-right').click();
    expect(await page.getByTestId('val-left').textContent()).toBe('left:1');
    expect(await page.getByTestId('val-right').textContent()).toBe('right:0');
    expect(await page.getByTestId('val-right').evaluate(el => getComputedStyle(el).color)).toBe('rgb(17, 17, 17)');

    const report = await page.evaluate(async () => {
      const { reply } = await import('/perf.js');
      return reply(document.getElementById('exact-root'), {});
    });
    const bySite = new Map(report.sites.map(s => [s.site, s]));
    expect(bySite.get(left.val).evaluated).toBeGreaterThanOrEqual(1);
    expect(bySite.get(right.val).evaluated).toBeGreaterThanOrEqual(1);
    expect(bySite.get(left.card).live).toBe(1);
    expect(bySite.get(right.card).live).toBe(1);
    expect(bySite.get(left.optX).live).toBe(2);
    expect(bySite.get(right.optX).live).toBe(2);
  }, ['--sites', '--dev-reload']);
}, 60000);
