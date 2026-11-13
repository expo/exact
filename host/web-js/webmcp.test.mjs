import { expect, test } from 'bun:test';
import { cpSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// calls.js's WebMCP binding (LLP 1133.023 D8) against a fake `document.modelContext`
// (https://webmachinelearning.github.io/webmcp/, §4.2 registerTool), over a stub runtime: calls.js is copied beside
// an `rt.js` and a `held.js` of its own, so each test has a fresh module, as in scheduler.test.mjs.
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

async function load(registerTool) {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-webmcp-'));
  cpSync(resolve(ROOT, 'host/web-js/calls.js'), resolve(dir, 'calls.js'));
  writeFileSync(resolve(dir, 'rt.js'), `export const clock={now:0,agent:false,timers:[]},data={},After=[],Clocked=[],Mutations=[],Origin={now:()=>'app'},journal=[],say=x=>journal.push(x),order=()=>0,drive=()=>{},time=()=>{};`);
  writeFileSync(resolve(dir, 'held.js'), 'export const Counting=new Set(),heldTickets=()=>[];');
  globalThis.document = { modelContext: { registerTool } };
  globalThis.exact = {};
  const at = name => import(pathToFileURL(resolve(dir, name)).href);
  const [{ tools }, rt, held] = await Promise.all([at('calls.js'), at('rt.js'), at('held.js')]);
  const settle = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
  /** A commit's end: calls.js offers again after each. */
  const commit = async () => { for (const f of rt.After) f(); await settle(); };
  return { dir, tools, rt, held, settle, commit };
}

/** registerTool's calls, newest last; `answer` gives each its promise. */
function fake(answer = () => Promise.resolve(undefined)) {
  const registered = [];
  const registerTool = (tool, options) => { const r = { tool, options }; registered.push(r); return answer(r, registered.length); };
  return { registered, registerTool, named: name => registered.filter(r => r.tool.name === name) };
}

const face = (doors, extra = {}) => ({ digest: 'd1', doors, ...extra });
const flags = [false, false, true];

test('faced tools on the web door are registered, described, withdrawn while unavailable, and run as calls', async () => {
  const old = { document: globalThis.document, exact: globalThis.exact };
  const browser = fake();
  const { dir, tools, rt, settle, commit } = await load(browser.registerTool);
  try {
    let open = true, waited = [];
    tools({
      waitFor: [() => (ms, size) => waited.push([ms, size]), { ms: 'n', size: ['e', ['small', 'large']] }, 'writes', flags,
        'Wait for ms milliseconds.', null, () => waited.length, () => open,
        face(['apple', 'android', 'web'], { title: 'Wait For', parameters: { ms: { title: 'Milliseconds', type: 'integer', description: 'How long, in milliseconds.' },
          size: { title: 'Size' } } }), 'n'],
      look: [() => () => {}, { q: 's' }, 'reads', flags, 'Look something up.', null, null, null, face(undefined), null],
      broken: [() => () => {}, {}, 'destroys', flags, 'Always fails.', () => true, null, null, face(['web']), null],
      note: [() => () => {}, {}, 'writes', [true, false, true], 'Add a note.', null, () => 'Rain at noon', null, face(['web']), 's'],
      native: [() => () => {}, {}, 'writes', flags, 'Native only.', null, null, null, face(['apple']), null],
      bare: [() => () => {}, {}, 'writes', flags, 'No face.', null, null, null, null, null],
    });
    await settle();
    expect(browser.registered.map(r => r.tool.name)).toEqual(['waitFor', 'look', 'broken', 'note']);
    expect(rt.journal.filter(l => l.startsWith('webmcp'))).toEqual(['webmcp waitFor: registered', 'webmcp look: registered', 'webmcp broken: registered',
      'webmcp note: registered']);

    const [wait] = browser.named('waitFor'), [look] = browser.named('look'), [broken] = browser.named('broken'), [note] = browser.named('note');
    expect(wait.tool).toMatchObject({
      title: 'Wait For', description: 'Wait for ms milliseconds.',
      inputSchema: { type: 'object', properties: { ms: { type: 'integer', title: 'Milliseconds', description: 'How long, in milliseconds.' },
        size: { type: 'string', enum: ['small', 'large'], title: 'Size' } }, required: ['ms', 'size'], additionalProperties: false },
      annotations: { readOnlyHint: false, consequentialHint: true, untrustedContentHint: false },
    });
    expect(look.tool).toMatchObject({ title: 'look', inputSchema: { properties: { q: { type: 'string' } }, required: ['q'] },
      annotations: { readOnlyHint: true, consequentialHint: false, untrustedContentHint: false } });
    expect(broken.tool.annotations).toEqual({ readOnlyHint: false, consequentialHint: true, untrustedContentHint: false });
    // A write that only adds to the app's own data is not consequential; a string it returns may carry text the
    // author did not write.
    expect(note.tool.annotations).toEqual({ readOnlyHint: false, consequentialHint: false, untrustedContentHint: true });

    // The output schema is the envelope, its value typed from `returns`; every result below is one.
    const envelope = { outcome: 'ok', certainty: 'applied', reason: '', stale: false, skipped: [], value: null, message: '' };
    expect(wait.tool.outputSchema).toMatchObject({ type: 'object', required: Object.keys(envelope), additionalProperties: false,
      properties: { value: { anyOf: [{ type: 'number' }, { type: 'null' }] }, certainty: { enum: ['none', 'applied', 'uncertain'] } } });
    expect(look.tool.outputSchema.properties.value).toEqual({ type: 'null' });
    expect(note.tool.outputSchema.properties.value.anyOf[0]).toEqual({ type: 'string' });
    expect(await wait.tool.execute({ ms: 250, size: 'small' })).toEqual({ ...envelope, value: 1 });
    expect(waited).toEqual([[250, 'small']]);
    expect(await look.tool.execute({ q: 'x' })).toEqual(envelope);
    expect(await note.tool.execute({})).toEqual({ ...envelope, value: 'Rain at noon' });
    expect(await broken.tool.execute({})).toEqual({ ...envelope, outcome: 'failed', certainty: 'uncertain',
      message: 'It may or may not have taken effect: read before calling again.' });
    expect(await wait.tool.execute({ ms: 'soon', size: 'small' })).toEqual({ ...envelope, outcome: 'refused', certainty: 'none', reason: 'arguments',
      message: 'ms: a finite number, not "soon"' });
    expect(globalThis.exact.calls.state().calls.map(c => [c.tool, c.door, c.outcome])).toEqual([
      ['waitFor', 'web', 'ok'], ['look', 'web', 'ok'], ['note', 'web', 'ok'], ['broken', 'web', 'failed'], ['waitFor', 'web', 'refused']]);

    open = false;
    await commit();
    expect(wait.options.signal.aborted).toBe(true);
    expect(rt.journal.at(-1)).toBe('webmcp waitFor: withdrawn');
    expect(await wait.tool.execute({ ms: 1, size: 'large' })).toMatchObject({ outcome: 'refused', reason: 'unavailable' });
    open = true;
    await commit();
    expect(browser.named('waitFor')).toHaveLength(2);
    expect(browser.named('waitFor')[1].options.signal.aborted).toBe(false);
    expect(rt.journal.at(-1)).toBe('webmcp waitFor: registered');
  } finally {
    Object.assign(globalThis, old);
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a refused registration is journalled, and offered again only after the tool has been unavailable', async () => {
  const old = { document: globalThis.document, exact: globalThis.exact };
  const unhandled = [], listen = e => unhandled.push(e);
  process.on('unhandledRejection', listen);
  // The first registration is refused, as Chrome refuses a duplicate name; later ones are accepted at once, as
  // Chrome accepts them.
  const browser = fake((_, n) => n === 1 ? Promise.reject(new DOMException('Duplicate tool name', 'InvalidStateError')) : Promise.resolve(undefined));
  const { dir, tools, rt, settle, commit } = await load(browser.registerTool);
  const lines = () => rt.journal.filter(l => l.startsWith('webmcp'));
  try {
    let open = true;
    tools({ look: [() => () => {}, {}, 'reads', flags, 'Look.', null, null, () => open, face(['web']), null] });
    await settle();
    expect(lines()).toEqual(['webmcp look: refused: InvalidStateError: Duplicate tool name']);
    // A refusal that stands is not retried at every commit.
    await commit();
    await commit();
    expect(browser.named('look')).toHaveLength(1);
    expect(lines()).toHaveLength(1);
    // Once the tool has been unavailable, it is offered again.
    open = false;
    await commit();
    open = true;
    await commit();
    expect(browser.named('look')).toHaveLength(2);
    expect(lines().at(-1)).toBe('webmcp look: registered');
    // A withdrawal aborts the signal and is not a refusal.
    open = false;
    await commit();
    expect(browser.named('look')[1].options.signal.aborted).toBe(true);
    expect(lines().slice(-2)).toEqual(['webmcp look: registered', 'webmcp look: withdrawn']);
    expect(unhandled).toEqual([]);
  } finally {
    process.off('unhandledRejection', listen);
    Object.assign(globalThis, old);
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a call its caller aborts while it waits never runs; one aborted after dispatch runs to its end', async () => {
  const old = { document: globalThis.document, exact: globalThis.exact };
  const browser = fake();
  const { dir, tools, rt, held, settle, commit } = await load(browser.registerTool);
  try {
    // `slow` owns a request until the test lets it go; `quick` records that it ran.
    const ticket = {}, ran = [];
    tools({
      slow: [() => () => held.Counting.add(ticket), {}, 'writes', flags, 'Slow.', null, () => 'done', null, face(['web']), 's'],
      quick: [() => () => ran.push('quick'), {}, 'writes', flags, 'Quick.', null, null, null, face(['web']), null],
    });
    await settle();
    const [slow] = browser.named('slow'), [quick] = browser.named('quick');
    const finish = async () => { held.Counting.delete(ticket); await commit(); };
    const record = id => globalThis.exact.calls.state().calls.find(c => c.id === id);

    const first = slow.tool.execute({}, { signal: new AbortController().signal });
    await settle();
    const waits = new AbortController(), second = quick.tool.execute({}, { signal: waits.signal });
    await settle();
    expect(globalThis.exact.calls.state().calls.map(c => [c.tool, c.state])).toEqual([['slow', 'running'], ['quick', 'waiting']]);
    waits.abort();
    expect(await second).toEqual({ outcome: 'refused', certainty: 'none', reason: 'cancelled', stale: false, skipped: [], value: null,
      message: 'Its caller cancelled it before it ran.' });
    expect(record(2)).toMatchObject({ outcome: 'refused', reason: 'cancelled', cancelled: true });
    expect(rt.journal).toContain('tool:system:web quick: cancelled before it ran');
    await finish();
    expect(await first).toMatchObject({ outcome: 'ok', certainty: 'applied', value: 'done' });
    expect(ran).toEqual([]);

    // Dispatched, the call's commit has landed: an abort leaves it to finish, and its record says the caller left.
    const runs = new AbortController(), third = slow.tool.execute({}, { signal: runs.signal });
    await settle();
    expect(record(3).state).toBe('running');
    runs.abort();
    expect(globalThis.exact.calls.cancel(3)).toEqual({ call: 3, state: 'running' });
    expect(rt.journal.filter(l => l.includes('its caller cancelled'))).toEqual(['tool:system:web slow: its caller cancelled; it runs to its end']);
    await finish();
    expect(await third).toMatchObject({ outcome: 'ok', certainty: 'applied', value: 'done' });
    expect(record(3)).toMatchObject({ outcome: 'ok', cancelled: true });
    expect(globalThis.exact.calls.cancel(3)).toEqual({ call: 3, state: 'ended' });
    expect(globalThis.exact.calls.cancel(9)).toEqual({ error: 'no call 9' });
  } finally {
    Object.assign(globalThis, old);
    rmSync(dir, { recursive: true, force: true });
  }
});

test('only a registration still held is journalled, and a withdrawal\'s own rejection is no refusal', async () => {
  const old = { document: globalThis.document, exact: globalThis.exact };
  const unhandled = [], listen = e => unhandled.push(e);
  process.on('unhandledRejection', listen);
  try {
    // The browser accepts at once, and the tool is withdrawn before that answer is heard.
    {
      const browser = fake();
      const { dir, tools, rt } = await load(browser.registerTool);
      try {
        let open = true;
        tools({ look: [() => () => {}, {}, 'reads', flags, 'Look.', null, null, () => open, face(['web']), null] });
        await Promise.resolve();
        expect(browser.named('look')).toHaveLength(1);
        open = false;
        for (const f of rt.After) f();
        for (let i = 0; i < 8; i++) await Promise.resolve();
        expect(browser.named('look')[0].options.signal.aborted).toBe(true);
        expect(rt.journal.filter(l => l.startsWith('webmcp'))).toEqual(['webmcp look: withdrawn']);
      } finally { rmSync(dir, { recursive: true, force: true }); }
    }
    // The browser has not answered when the tool is withdrawn, and rejects with the signal's reason (§4.2,
    // registerTool's abort steps): that is the withdrawal, not a refusal, and the tool is offered again when available.
    {
      const browser = fake(({ options }) => new Promise((_, reject) => options.signal.addEventListener('abort', () => reject(options.signal.reason))));
      const { dir, tools, rt, settle, commit } = await load(browser.registerTool);
      try {
        let open = true;
        tools({ look: [() => () => {}, {}, 'reads', flags, 'Look.', null, null, () => open, face(['web']), null] });
        await settle();
        open = false;
        await commit();
        open = true;
        await commit();
        expect(browser.named('look')).toHaveLength(2);
        expect(rt.journal.filter(l => l.startsWith('webmcp'))).toEqual(['webmcp look: withdrawn']);
      } finally { rmSync(dir, { recursive: true, force: true }); }
    }
    expect(unhandled).toEqual([]);
  } finally {
    process.off('unhandledRejection', listen);
    Object.assign(globalThis, old);
  }
});

test('a reads tool whose reach sends is not read-only (LLP 1111 D3)', async () => {
  const old = { document: globalThis.document, exact: globalThis.exact };
  const browser = fake();
  const { dir, tools, settle } = await load(browser.registerTool);
  try {
    tools({
      look: [() => () => {}, {}, 'reads', [false, false, true, false], 'Look.', null, null, null, face(['web']), null],
      search: [() => () => {}, {}, 'reads', [false, false, true, true], 'Search.', null, null, null, face(['web']), null],
    });
    await settle();
    expect(browser.registered.map(r => [r.tool.name, r.tool.annotations.readOnlyHint])).toEqual([['look', true], ['search', false]]);
  } finally {
    Object.assign(globalThis, old);
    rmSync(dir, { recursive: true, force: true });
  }
});
