import { test, expect } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { webCompiler } from './module.mjs';

// A child's sends on the JS target (LLP 1035.005.004 D6), as the runner's
// runner/tests/it/owned_sends.rs drives them: each instance of a component
// that declares a mutation sends on its own, its `then` runs in its frames,
// and its end forgets what it sent. The clock is the agent's.

const root = resolve(import.meta.dir, '../..');

// `detect(id)`, `save(id)` and `notes(id)` answer when the test lands them,
// `quick(id)` at once, and `bad(id)` at once outside its shape; every answer's
// lines name what was asked, and every ask is in `globalThis.asks`.
const appModule = `export const appId = 'test.sends';
export const grants = '';
const g = globalThis as any;
g.asks ??= []; g.held ??= [];
export function answer(name: string, args: any): any {
  const id = String(args[0] ?? '');
  g.asks.push(name + '(' + id + ')');
  if (name === 'detect' || name === 'save' || name === 'notes')
    return new Promise(done => g.held.push({ name, id, done: () => done({ lines: [name + ' ' + id] }) }));
  if (name === 'quick') return { lines: ['quick ' + id] };
  if (name === 'bad') return 1;
  throw new Error('no source ' + name);
}
`;

const BOOKS = `routes nav
  list "/"
  book "/book/:id"

shape Lines
  lines: string[]

component App
  state shown = ""
  action open(id: string)
    nav = push(nav, path("book", id))
  action back
    nav = back(nav)
  action showLines(lines: string[])
    shown = lines.join(",")
  view
    main
      text shown testId="shown"
      button press=open("1") testId="open-1"
        text "1"
      button press=open("2") testId="open-2"
        text "2"
      button press=back testId="back"
        text "Back"
      for e in stack(nav) key=e.id
        if e.name == "book"
          Book(id=e.params.id, show=showLines)
        else
          text "List" testId="list"

component Book
  props
    id: string
    show: action
  resource notes = notes(id) as shape Lines else empty()
  mutation detected as shape Lines then got
  mutation saved as shape Lines queue refreshes notes
  action detect
    send detected = detect(id)
  action detectNow
    send detected = quick(id)
  action broken
    send detected = bad(id)
  action clear
    detected = none
  action got
    match detected
      case some(d)
        show(d.lines)
      case none
        show([])
  action save
    send saved = save(id)
  view
    column
      text (pending(detected) ? "detecting" : match detected { case some(d) => d.lines.join(","), case none => "idle" }) testId=\`state-\${id}\`
      text (pending(saved) ? "saving" : "saved") testId=\`saving-\${id}\`
      text notes.lines.join(",") testId=\`notes-\${id}\`
      button press=detect testId=\`detect-\${id}\`
        text "Detect"
      button press=detectNow testId=\`now-\${id}\`
        text "Now"
      button press=broken testId=\`bad-\${id}\`
        text "Bad"
      button press=clear testId=\`clear-\${id}\`
        text "Clear"
      button press=save testId=\`save-\${id}\`
        text "Save"
`;

/** Builds `source` for the web, opens it and hands `run` the page and its helpers. */
async function fixture(source, run) {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/owned-sends-test-'));
  // open() resolves a path through EXACT_APP_DIR, which is shared across test files.
  const appDir = process.env.EXACT_APP_DIR;
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Sends', app: { id: 'test.sends', name: 'Sends' } }));
    writeFileSync(resolve(dir, 'app.contract'), source);
    writeFileSync(resolve(dir, 'app.ts'), appModule);
    const dist = resolve(dir, 'dist');
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'sends', '--out', dist, '--render', 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const p = new URL(req.url).pathname;
      if (p.startsWith('/shared/')) return new Response(Bun.file(resolve(root, 'host/web', p.slice('/shared/'.length))));
      const file = resolve(dist, p.slice(1));
      if (existsSync(file) && p.includes('.')) return new Response(Bun.file(file));
      return new Response(Bun.file(resolve(dist, 'shell.html')));
    } });
    browser = await open({ host: 'web', url: new URL('/', server.url).href, webDist: dist });
    const text = async id => (await browser.tree()).nodes.find(n => n.props.testId === id)?.props.text ?? null;
    const asks = () => browser.carrier.evaluate('globalThis.asks ?? []');
    // Answers the held asks `which` picks (every one by default) and, once their replies have committed (the task
    // after their promises'), lands their `then`s, the clock unmoved (Runner::land_then); how many. A settle would
    // wait on the asks still held.
    const land = async (which = '() => true') => {
      const n = await browser.carrier.evaluate(`(() => {
        const pick = ${which}, h = globalThis.held, mine = h.filter(pick);
        globalThis.held = h.filter(x => !mine.includes(x));
        mine.forEach(x => x.done());
        return new Promise(done => setTimeout(() => done(mine.length)));
      })()`);
      await thens();
      return n;
    };
    const thens = async () => expect((await browser.carrier.ask({ op: 'clock', land: true })).error).toBeUndefined();
    // Opens book `id` and lands its notes.
    const openBook = async id => {
      await browser.tap(`open-${id}`);
      await land("x => x.name === 'notes'");
    };
    // The mutations holding a ticket, by name: a send forgotten holds none.
    const held = () => browser.carrier.evaluate('globalThis.exact.mutations.filter(m => m.ticket || m.tickets.length).map(m => m.name)');
    await run({ browser, text, asks, land, thens, openBook, held });
  } finally {
    if (appDir === undefined) delete process.env.EXACT_APP_DIR;
    else process.env.EXACT_APP_DIR = appDir;
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}

test("a screen's then runs in its frame and hands the lines to the root", () => fixture(BOOKS, async ({ browser, text, land, openBook }) => {
  await openBook('1');
  expect(await text('notes-1')).toBe('notes 1');
  await browser.tap('detect-1');
  expect(await text('state-1')).toBe('detecting');
  expect(await land()).toBe(1);
  // The `then` is its own commit: it reads the instance's answer and calls the root's action with it.
  expect(await text('state-1')).toBe('detect 1');
  expect(await text('shown')).toBe('detect 1');

  // An answer at once lands in the instance, and its `then` follows, landed with the input.
  await browser.tap('now-1');
  expect(await text('state-1')).toBe('quick 1');
  expect(await text('shown')).toBe('quick 1');
  // The instance's own assignment clears its answer.
  await browser.tap('clear-1');
  expect(await text('state-1')).toBe('idle');
}), 180_000);

test('leaving the screen before its answer forgets the send and runs no then', () => fixture(BOOKS, async ({ browser, text, land, openBook, held }) => {
  await openBook('1');
  await browser.tap('detect-1');
  expect(await held()).toEqual(['detected']);
  await browser.tap('back');
  // The send was made and stays made; its ticket is forgotten, so its answer changes nothing.
  expect(await held()).toEqual([]);
  expect((await browser.logs()).lines.some(l => /forget ticket \d+ \(detected\)/.test(l))).toBe(true);
  expect(await land()).toBe(1);
  expect((await browser.logs()).lines.some(l => /dropped reply for detected: ticket \d+ is no longer held/.test(l))).toBe(true);
  expect(await text('shown')).toBe('');

  // The same book opened again is a new instance: nothing of its predecessor's send reaches it.
  await openBook('1');
  expect(await text('state-1')).toBe('idle');
  expect(await land()).toBe(0);
  expect(await text('state-1')).toBe('idle');
  expect(await text('shown')).toBe('');
}), 180_000);

test('a covered screen keeps its instance and its then runs', () => fixture(BOOKS, async ({ browser, text, land, openBook }) => {
  await openBook('1');
  await browser.tap('detect-1');
  // A push covers the screen; it stays, so its send's `then` still runs.
  await openBook('2');
  expect(await text('state-2')).toBe('idle');
  expect(await text('state-1')).toBe('detecting');
  expect(await land("x => x.name === 'detect'")).toBe(1);
  expect(await text('shown')).toBe('detect 1');
  expect(await text('state-1')).toBe('detect 1');
  expect(await text('state-2')).toBe('idle');
}), 180_000);

test('two screens send apart', () => fixture(BOOKS, async ({ browser, text, asks, land, openBook, held }) => {
  await openBook('1');
  await openBook('2');
  await browser.tap('detect-1');
  await browser.tap('detect-2');
  // One send each: neither replaces the other.
  expect((await asks()).filter(a => a.startsWith('detect'))).toEqual(['detect(1)', 'detect(2)']);
  expect(await held()).toEqual(['detected', 'detected']);
  expect(await text('state-1')).toBe('detecting');
  expect(await text('state-2')).toBe('detecting');
  expect(await land("x => x.id === '2'")).toBe(1);
  expect(await text('state-1')).toBe('detecting');
  expect(await text('state-2')).toBe('detect 2');
  expect(await text('shown')).toBe('detect 2');
}), 180_000);

test("a screen's queue refreshes its own notes and is dropped with it", () => fixture(BOOKS, async ({ browser, text, asks, land, openBook, held }) => {
  await openBook('1');
  await openBook('2');
  // Its reply refreshes this instance's notes only.
  await browser.tap('save-2');
  expect(await text('saving-2')).toBe('saving');
  expect(await text('saving-1')).toBe('saved');
  expect(await land("x => x.name === 'save'")).toBe(1);
  expect((await asks()).filter(a => a.startsWith('notes'))).toEqual(['notes(1)', 'notes(2)', 'notes(2)']);
  expect(await land()).toBe(1);
  expect(await text('saving-2')).toBe('saved');
  // Three saves: one in flight, two waiting their turn. Back destroys the screen, and the waiting saves go with it.
  for (let k = 0; k < 3; k++) await browser.tap('save-2');
  expect((await browser.state()).queued).toEqual({ saved: 2 });
  await browser.tap('back');
  expect((await browser.state()).queued).toEqual({});
  expect(await held()).toEqual([]);
  const lines = (await browser.logs()).lines;
  expect(lines.some(l => l.includes('forgot 2 waiting sends (saved)'))).toBe(true);
  expect(lines.some(l => /forget ticket \d+ \(saved\)/.test(l))).toBe(true);
  // The save in flight was made; its reply asks nothing more.
  expect(await land()).toBe(1);
  expect((await asks()).filter(a => a.startsWith('save'))).toEqual(['save(2)', 'save(2)']);
  expect((await asks()).filter(a => a.startsWith('notes')).length).toBe(3);
  expect(await text('saving-1')).toBe('saved');
}), 180_000);

test('a use outside every region sends as the root does', () => fixture(BOOKS.replace(`      for e in stack(nav) key=e.id
        if e.name == "book"
          Book(id=e.params.id, show=showLines)
        else
          text "List" testId="list"
`, '      Book(id="9", show=showLines)\n'), async ({ browser, text, land }) => {
  await land();
  await browser.tap('detect-9');
  expect(await text('state-9')).toBe('detecting');
  await land();
  expect(await text('state-9')).toBe('detect 9');
  expect(await text('shown')).toBe('detect 9');
}), 180_000);

test('a refused send leaves the instance as it was', () => fixture(BOOKS, async ({ browser, text, land, openBook, held }) => {
  await openBook('1');
  await openBook('2');
  // An answer outside its shape: the app module's answer is checked as it arrives, so on this target the send fails
  // (the runner refuses the action), and the instance's answer is as it was.
  await browser.tap('bad-2');
  expect((await browser.logs()).lines.some(l => l.includes('send detected failed: `bad` answered outside its shape'))).toBe(true);
  expect(await text('state-2')).toBe('idle');
  // Both screens still send, each on its own.
  await browser.tap('detect-2');
  await browser.tap('back');
  await browser.tap('detect-1');
  expect(await text('state-1')).toBe('detecting');
  // The popped screen's send is forgotten.
  expect(await held()).toEqual(['detected']);
  expect(await land("x => x.id === '1'")).toBe(1);
  expect(await text('state-1')).toBe('detect 1');
}), 180_000);
