import { test, expect } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { webCompiler } from './module.mjs';

// A child's requests on the JS target (LLP 1035.005.004), as the runner's
// runner/tests/it/owned.rs drives them: each instance of a component that
// declares a resource owns its own request, made with the instance and let
// go when it ends.

const root = resolve(import.meta.dir, '../..');

test('equal-deadline child tasks follow instance creation order across recreation and rearming', async () => {
  const source = readFileSync(resolve(root, 'semantics/corpus/timers/owned-order.contract'), 'utf8');
  for (const change of ['toggle', 'gate', 'rekey']) await fixture(source, async ({ browser, text }) => {
    await browser.tap('b');
    await browser.tap(change);
    if (change !== 'rekey') await browser.tap(change);
    await browser.clock('+10');
    expect(await text('total')).toBe(change === 'toggle' ? '21' : '12');
  });
}, 180_000);

// `subjectWorks(s)` answers when the test lands it (`hold`) or at once, with `s`
// as its one work; `now(x)` answers at once; every ask is recorded in
// `globalThis.asks`.
const appModule = (hold = true) => `export const appId = 'test.owned';
export const grants = '';
const g = globalThis as any;
g.asks ??= []; g.held ??= [];
export function answer(name: string, args: any): any {
  const a = String(args[0] ?? '');
  g.asks.push(name + '(' + a + ')');
  if (name === 'subjectWorks') return ${hold} ? new Promise(done => g.held.push(() => done({ ok: true, works: [a] }))) : Promise.resolve({ ok: true, works: [a] });
  if (name === 'now') return 'now ' + a;
  if (name === 'mirror') return args[0];
  if (name === 'one') return g.__exactRender ? 1 : new Promise(done => g.held.push(() => done(1)));
  if (name === 'emptyWorks') return { ok: true, works: ['ph'] };
  throw new Error('no source ' + name);
}
`;

const DISCOVER = `routes nav
  tab home "/"
  tab discover "/discover"

shape Subject
  ok: bool
  works: string[]

component App
  action pick(name: string)
    nav = select(nav, name)
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back"
      column
        for t in nav.tabs key=t.name
          column role="tabpanel" id=\`panel-\${t.name}\` testId=\`panel-\${t.name}\`
            for e in t.stack key=e.id
              column navigationKey=\`\${e.id}\`
                if e.name == "discover"
                  Discover(shown=(nav.tab == t.name))
                else
                  text "Home" testId="home"
      row role="tablist"
        button press=pick("home") testId="tab-home" role="tab" aria-controls="panel-home" aria-selected=(nav.tab == "home")
          text "Home"
        button press=pick("discover") testId="tab-discover" role="tab" aria-controls="panel-discover" aria-selected=(nav.tab == "discover")
          text "Discover"

component Discover
  props
    shown: bool
  state subject = "sf"
  resource works = subjectWorks(subject) when shown as shape Subject else empty(ok=true)
  action choose(s: string)
    subject = s
  action reload
    refresh works
  view
    column
      text (pending(works) ? \`loading \${works.works.join(",")}\` : works.works.join(",")) testId="works"
      button press=choose("fantasy") testId="fantasy"
        text "Fantasy"
      button press=reload testId="reload"
        text "Reload"
`;

const BOOKS = `routes nav
  list "/"
  book "/book/:id"

shape Subject
  ok: bool
  works: string[]

component App
  action open(id: string)
    nav = push(nav, path("book", id))
  action swap(id: string)
    nav = replace(nav, path("book", id))
  action back
    nav = back(nav)
  view
    main
      button press=open("1") testId="open-1"
        text "1"
      button press=open("2") testId="open-2"
        text "2"
      button press=swap("2") testId="swap-2"
        text "Swap"
      button press=back testId="back"
        text "Back"
      for e in stack(nav) key=e.id
        if e.name == "book"
          Book(id=e.params.id)
        else
          text "List" testId="list"

component Book
  props
    id: string
  resource works = subjectWorks(id) as shape Subject else empty()
  action reload
    refresh works
  view
    column
      text \`\${id}: \${works.works.join(",")}\` testId=\`book-\${id}\`
      button press=reload testId=\`reload-\${id}\`
        text "Reload"
`;

const CHAIN = `routes nav
  home "/"
  scan "/scan"

component App
  action scan
    nav = push(nav, "/scan")
  view
    main
      button press=scan testId="scan-open"
        text "Scan"
      for e in stack(nav) key=e.id
        if e.name == "scan"
          Scan()
        else
          text "Home"

component Scan
  state code = "978"
  resource scanned = now(code) as shape string
  derive isbn = scanned == "" ? "" : \`\${scanned}!\`
  resource lookup = now(isbn) as shape string
  view
    text \`\${scanned} / \${lookup}\` testId="scan"
`;

const FEED = `component App
  view
    main
      Feed(topic="t")

component Feed
  props
    topic: string
  state n = 1
  resource got = now(\`\${topic}\${n}\`) as shape string
  action more
    n = n + 1
  view
    column
      text got testId="got"
      button press=more testId="more"
        text "More"
`;

/** Builds `source` for the web, opens it at `path` and hands `run` the page
 * and its helpers; `render` renders its `render=build` pages. */
async function fixture(source, run, { path = '/', render = false, hold = true, stored } = {}) {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/owned-test-'));
  // open() resolves a path through EXACT_APP_DIR, which is shared across test files.
  const appDir = process.env.EXACT_APP_DIR;
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Owned', app: { id: 'test.owned', name: 'Owned' } }));
    writeFileSync(resolve(dir, 'app.contract'), source);
    writeFileSync(resolve(dir, 'app.ts'), appModule(hold));
    const dist = resolve(dir, 'dist');
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'owned', '--out', dist, '--render', render ? 'js' : 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const p = new URL(req.url).pathname;
      if (p.startsWith('/shared/')) return new Response(Bun.file(resolve(root, 'host/web', p.slice('/shared/'.length))));
      const file = resolve(dist, p.slice(1)), page = resolve(dist, p.slice(1), 'index.html');
      if (existsSync(file) && p.includes('.')) return new Response(Bun.file(file));
      return new Response(Bun.file(existsSync(page) ? page : resolve(dist, 'shell.html')));
    } });
    const env = stored && { EXACT_AGENT_STORAGE_FRESH: '1', EXACT_AGENT_STORED: JSON.stringify(stored) };
    browser = await open({ host: 'web', url: new URL(path, server.url).href, webDist: dist, env });
    const text = async id => (await browser.tree()).nodes.find(n => n.props.testId === id)?.props.text ?? null;
    const asks = () => browser.carrier.evaluate('globalThis.asks ?? []');
    // Answers every request out; how many there were.
    const land = async () => {
      const n = await browser.carrier.evaluate('(() => { const h = (globalThis.held ?? []).splice(0); h.forEach(f => f()); return h.length; })()');
      await browser.clock('settle');
      return n;
    };
    await run({ browser, text, asks, land });
  } finally {
    if (appDir === undefined) delete process.env.EXACT_APP_DIR;
    else process.env.EXACT_APP_DIR = appDir;
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}

test('a tab screen asks while shown and keeps its answer while hidden', () => fixture(DISCOVER, async ({ browser, text, asks, land }) => {
  expect(await asks()).toEqual([]);
  expect(await text('works')).toBe(null);

  await browser.tap('tab-discover');
  expect(await asks()).toEqual(['subjectWorks(sf)']);
  expect(await text('works')).toBe('loading ');
  expect(await land()).toBe(1);
  expect(await text('works')).toBe('sf');

  // The agent lists it by instance path, apart from the root's resources.
  const state = await browser.state();
  expect(state.owned).toEqual(['works']);
  expect(Object.keys(state.resources).filter(name => name.split('#')[0] === 'works')).toEqual([]);
  expect(Object.values(state.instances)).toEqual([{ works: { ok: true, works: ['sf'] } }]);

  await browser.tap('tab-home');
  expect((await asks()).length).toBe(1);
  expect(await text('works')).toBe('sf');
  expect(await land()).toBe(0);

  await browser.tap('tab-discover');
  expect(await asks()).toEqual(['subjectWorks(sf)', 'subjectWorks(sf)']);
  expect(await text('works')).toBe('loading sf');
  await land();

  await browser.tap('fantasy');
  expect((await asks()).at(-1)).toBe('subjectWorks(fantasy)');
  await land();
  expect(await text('works')).toBe('fantasy');
  await browser.tap('reload');
  expect((await asks()).length).toBe(4);
  expect(await text('works')).toBe('loading fantasy');
  await land();
  expect(await text('works')).toBe('fantasy');

  // `perf` counts every instance's asks under the declared name.
  const perf = await browser.perf();
  expect(perf.resources.find(r => r.name === 'works')?.asked).toBe(4);
}), 180_000);

test('a popped screen lets its request go and its late answer changes nothing', () => fixture(BOOKS, async ({ browser, text, asks, land }) => {
  await browser.tap('open-1');
  expect(await asks()).toEqual(['subjectWorks(1)']);
  // Keep the first answer back past the screen's end.
  await browser.carrier.evaluate('globalThis.first = globalThis.held.splice(0)');
  await browser.tap('back');
  expect(await text('book-1')).toBe(null);
  await browser.tap('open-1');
  expect((await asks()).length).toBe(2);
  expect(await text('book-1')).toBe('1: ');
  await browser.carrier.evaluate('globalThis.first.forEach(f => f())');
  await browser.clock('settle');
  expect(await text('book-1')).toBe('1: ');
  expect(await land()).toBe(1);
  expect(await text('book-1')).toBe('1: 1');
}), 180_000);

test('replace keeps the screen and asks with the new parameters', () => fixture(BOOKS, async ({ browser, text, asks, land }) => {
  await browser.tap('open-1');
  await land();
  await browser.tap('swap-2');
  expect(await asks()).toEqual(['subjectWorks(1)', 'subjectWorks(2)']);
  expect(await text('book-2')).toBe('2: 1');
  await land();
  expect(await text('book-2')).toBe('2: 2');
}), 180_000);

test('two instances ask and refresh apart', () => fixture(BOOKS, async ({ browser, text, asks, land }) => {
  for (const id of ['1', '2', '1']) await browser.tap(`open-${id}`);
  expect(await asks()).toEqual(['subjectWorks(1)', 'subjectWorks(2)', 'subjectWorks(1)']);
  const paths = Object.keys((await browser.state()).instances);
  expect(paths.length).toBe(3);
  expect(await land()).toBe(3);
  // The covered screen's reload reaches only it.
  const reloads = (await browser.tree()).nodes.filter(n => n.props.testId === 'reload-1');
  expect(reloads.length).toBe(2);
  await browser.tap(reloads[0].id);
  expect((await asks()).length).toBe(4);
  expect((await browser.state()).pending.map(p => p.name)).toEqual(['works']);
}), 180_000);

test('an unopened tab asks nothing and a shown one asks at that render', () => fixture(DISCOVER.replace('when shown ', ''), async ({ browser, asks }) => {
  expect(await asks()).toEqual([]);
  await browser.tap('tab-discover');
  expect(await asks()).toEqual(['subjectWorks(sf)']);
}), 180_000);

test('an immediate answer lands and what reads it asks then', () => fixture(CHAIN, async ({ browser, text, asks }) => {
  await browser.tap('scan-open');
  await browser.clock('settle');
  const seen = await asks();
  expect(seen[0]).toBe('now(978)');
  expect(seen.at(-1)).toBe('now(now 978!)');
  expect(await text('scan')).toBe('now 978 / now now 978!');
}), 180_000);

test('a use outside every region asks with the root', () => fixture(FEED, async ({ browser, text, asks }) => {
  await browser.clock('settle');
  expect(await asks()).toEqual(['now(t1)']);
  expect(await text('got')).toBe('now t1');
  await browser.tap('more');
  await browser.clock('settle');
  expect(await asks()).toEqual(['now(t1)', 'now(t2)']);
  expect(await text('got')).toBe('now t2');
}), 180_000);

// A child's state used outside every region starts as the root first renders, in the boot's commit (the runner's
// `init_late_slots`, LLP 1017 P4c): what its initializer asks, here a parent's resource read through a prop, lands.
const SEEDED = `shape Subject
  ok: bool
  works: string[]

component App
  resource base = subjectWorks("root") as shape Subject else empty()
  view
    main
      text base.works.join(",") testId="base"
      Seeded(seed=base)

component Seeded
  props
    seed: Subject
  state copy = seed
  view
    column
      text copy.works.join(",") testId="copy"
`;

test('a state used outside every region starts in the boot, and what it reads lands', () => fixture(SEEDED, async ({ text, asks, land }) => {
  expect(await asks()).toEqual(['subjectWorks(root)']);
  expect(await land()).toBe(1);
  // The state read the resource's placeholder as it started, once (LLP 1017.000 P4c)
  expect([await text('base'), await text('copy')]).toEqual(['root', '']);
}), 180_000);

test("an owned request's source placeholder shows in the making render", () => fixture(DISCOVER.replace('else empty(ok=true)', 'else emptyWorks()'), async ({ text, land }) => {
  expect(await text('works')).toBe('loading ph');
  expect(await land()).toBe(1);
  expect(await text('works')).toBe('sf');
}, { path: '/discover' }), 180_000);

test("a rendered page's instances answer the boot's first frame", () => fixture(DISCOVER.replace('tab discover "/discover"', 'tab discover "/discover" render=build'), async ({ browser, text, asks, land }) => {
  // The page holds the instance's answer: the boot shows it and asks nothing.
  expect(await text('works')).toBe('sf');
  expect(await asks()).toEqual([]);
  // An instance's later ask is its own.
  await browser.tap('fantasy');
  expect(await asks()).toEqual(['subjectWorks(fantasy)']);
  await land();
  expect(await text('works')).toBe('fantasy');
}, { path: '/discover', render: true, hold: false }), 180_000);

// A panel's feed asks while its tab shows; every panel's asks the same.
const PANELS = `routes nav
  tab home "/"
  tab discover "/discover" render=build

shape Subject
  ok: bool
  works: string[]

component App
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back"
      for t in nav.tabs key=t.name
        column role="tabpanel" id=\`panel-\${t.name}\`
          Feed(name=t.name, shown=(nav.tab == t.name))

component Feed
  props
    name: string
    shown: bool
  resource works = subjectWorks("sf") when shown as shape Subject else empty(ok=true)
  view
    text \`\${name}: \${works.works.join(",")}\` testId=\`works-\${name}\`
`;

test("a rendered page's instance takes its own answer, not a sibling's at equal arguments", () => fixture(PANELS, async ({ browser, text, asks }) => {
  // The page's home panel was rendered with its gate closed, so it shows its placeholder there and at the boot.
  expect([await text('works-home'), await text('works-discover')]).toEqual(['home: ', 'discover: sf']);
  expect(await asks()).toEqual([]);
}, { path: '/discover', render: true, hold: false }), 180_000);

test("a sleeping owned resource skips trapping arguments with only a sibling's answer", () => fixture(
  PANELS.replace('subjectWorks("sf")', 'subjectWorks(`${range(shown ? 1 : -1).length}`)'),
  async ({ text, asks }) => {
    expect([await text('works-home'), await text('works-discover')]).toEqual(['home: ', 'discover: 1']);
    expect(await asks()).toEqual([]);
  }, { path: '/discover', render: true, hold: false }), 180_000);

test('a sleeping owned resource boots before its dependency answers', () => fixture(`routes nav
  home "/" render=build
component App
  state show = true
  state input = 1 persist
  view
    column
      if show
        Chain(input=input)
component Chain
  props
    input: number
  resource first = one(input) as shape number
  resource second = mirror(range(first - 1).length + 7) when first > 0 as shape number
  view
    text \`\${first}:\${second}\` testId="answer"
`, async ({ text, asks, land }) => {
  expect(await text('answer')).toBe('0:0');
  expect(await asks()).toEqual(['one(2)']);
  expect(await land()).toBe(1);
  expect(await text('answer')).toBe('1:7');
  expect(await asks()).toEqual(['one(2)', 'mirror(7)']);
}, { render: true, stored: [['input', '2']] }), 180_000);

test('a sleeping owned resource discards an answer when persisted state breaks its arguments', () => fixture(`routes nav
  home "/" render=build
component App
  state show = true
  state number = 1 persist
  state forced = false
  action wake
    forced = true
  action repair
    number = 2
  view
    column
      button "Wake" press=wake testId="wake"
      button "Repair" press=repair testId="repair"
      if show
        Sleeping(number=number, awake=(number > 0 || forced))
component Sleeping
  props
    number: number
    awake: bool
  resource answer = mirror(range(number).length) when awake as shape number
  view
    text \`\${answer}:\${current(answer)}\` testId="answer"
`, async ({ browser, text, asks }) => {
  expect(await text('answer')).toBe('0:false');
  await browser.tap('wake');
  expect((await browser.state()).slots.forced).toBe(false);
  expect(await text('answer')).toBe('0:false');
  expect(await asks()).toEqual([]);
  await browser.tap('repair');
  expect(await text('answer')).toBe('2:true');
  expect(await asks()).toEqual(['mirror(2)']);
}, { render: true, stored: [['number', '-1']] }), 180_000);

const TASK_REFUSAL = `component App
  state shown = false
  state divisor = 0
  action show
    shown = true
  action repair
    divisor = 1
  view
    column
      text \`\${shown}\` testId="shown"
      button press=show testId="show"
        text "Show"
      button press=repair testId="repair"
        text "Repair"
      if shown
        Clock(divisor=divisor)
component Clock
  props
    divisor: number
  state count = 0
  task tick key=(1 / divisor)
    every(10, bump)
  action bump
    count = count + 1
  view
    text \`\${count}\` testId="clock"
`;
for (const fallback of [false, true]) {
  const source = fallback ? TASK_REFUSAL.replace('Clock(divisor=divisor)',
    'Clock(divisor=divisor)\n      else\n        Clock(divisor=1)') : TASK_REFUSAL;
  test(`a new child task refuses its mounting commit and recovers (old instance: ${fallback})`, () => fixture(source, async ({ browser, text }) => {
    await browser.clock('+10');
    await browser.tap('show');
    expect(await text('shown')).toBe('false');
    expect(await text('clock')).toBe(fallback ? '1' : null);
    expect((await browser.state()).slots.shown).toBe(false);
    await browser.clock('+10');
    expect(await text('clock')).toBe(fallback ? '2' : null);
    await browser.tap('repair');
    await browser.tap('show');
    await browser.clock('+10');
    expect(await text('clock')).toBe('1');
  }), 180_000);
}

test('gated and ungated tasks share declaration order', () => fixture(`component App
  state enabled = false
  state total = 0
  action enable
    enabled = true
  action first
    total = total * 10 + 1
  action second
    total = total * 10 + 2
  task gated when enabled
    every(10, first)
  task mounted mount
    every(10, second)
  view
    column
      text \`\${total}\` testId="total"
      button press=enable testId="enable"
        text "Enable"
`, async ({ browser, text }) => {
  await browser.tap('enable');
  await browser.clock('+10');
  expect(await text('total')).toBe('12');
}), 180_000);


test('an unopened tab refuses an invalid task key before changing selection', () => fixture(`routes nav
  tab a "/"
  tab b "/b"
component App
  state divisor = 1
  action pick(name: string)
    nav = select(nav, name)
  action breakKey
    divisor = 0
  action repair
    divisor = 1
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back"
      for t in nav.tabs key=t.name
        column role="tabpanel" id=\`panel-\${t.name}\`
          for e in t.stack key=e.id
            column navigationKey=\`\${e.id}\`
              Clock(name=t.name, divisor=t.name == "b" ? divisor : 1)
      row role="tablist"
        button press=pick("a") testId="tab-a" role="tab" aria-controls="panel-a" aria-selected=(nav.tab == "a")
          text "A"
        button press=pick("b") testId="tab-b" role="tab" aria-controls="panel-b" aria-selected=(nav.tab == "b")
          text "B"
      button press=breakKey testId="break"
        text "Break"
      button press=repair testId="repair"
        text "Repair"
      text nav.tab testId="selected"
component Clock
  props
    name: string
    divisor: number
  state count = 0
  task tick key=(1 / divisor)
    every(10, bump)
  action bump
    count = count + 1
  view
    text \`\${count}\` testId=\`clock-\${name}\`
`, async ({ browser, text }) => {
  expect(await text('clock-b')).toBe(null);
  await browser.tap('break');
  await browser.tap('tab-b');
  expect(await text('selected')).toBe('a');
  expect(await text('clock-b')).toBe(null);
  await browser.tap('repair');
  await browser.tap('tab-b');
  await browser.clock('+10');
  expect(await text('clock-b')).toBe('1');
}), 180_000);


test('destroying a child does not validate its departing task', () => fixture(TASK_REFUSAL
  .replace('state shown = false', 'state shown = true')
  .replace('state divisor = 0', 'state divisor = 1')
  .replace('shown = true\n  action repair', 'shown = false\n    divisor = 0\n  action repair'), async ({ browser, text }) => {
  await browser.clock('+10');
  await browser.tap('show');
  expect((await browser.state()).slots.shown).toBe(false);
  expect(await text('clock')).toBe(null);
  await browser.clock('+20');
  expect((await browser.state()).tasks).toEqual({});
}), 180_000);


test("a child's task key reads its synchronous resource before mounting", () => fixture(TASK_REFUSAL
  .replace('Clock(divisor=divisor)', 'Clock()')
  .replace('  props\n    divisor: number', '  resource n = now("x") as shape string')
  .replace('key=(1 / divisor)', 'key=(1 / n.length)'), async ({ browser, text, asks }) => {
  await browser.tap('show');
  expect((await browser.state()).slots.shown).toBe(true);
  expect(await asks()).toEqual(['now(x)']);
  await browser.clock('+10');
  expect(await text('clock')).toBe('1');
}), 180_000);

test('a virtualized list inside a child component mounts and updates when its owned resource settles', () => fixture(`shape Subject
  ok: bool
  works: string[]

component App
  state show = true
  view
    column
      if show
        Screen()

component Screen
  resource feed = subjectWorks("sf") as shape Subject else empty(ok=true)
  state ids = [1, 2]
  view
    list virtualized=true height=200
      for x in ids key=x
        text \`\${feed.works.join(",") }:\${pending(feed)}:\${failed(feed)}:\${x}\` testId=\`row-\${x}\`
`, async ({ text, asks, land }) => {
  expect(await asks()).toEqual(['subjectWorks(sf)']);
  expect([await text('row-1'), await text('row-2')]).toEqual([':true:false:1', ':true:false:2']);
  expect(await land()).toBe(1);
  expect([await text('row-1'), await text('row-2')]).toEqual(['sf:false:false:1', 'sf:false:false:2']);
}), 180_000);
