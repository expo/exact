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
        face(['apple', 'android', 'web'], { title: 'Wait For', parameters: { ms: { title: 'Milliseconds', type: 'integer' }, size: { title: 'Size' } } })],
      look: [() => () => {}, { q: 's' }, 'reads', flags, 'Look something up.', null, null, null, face(undefined)],
      broken: [() => () => {}, {}, 'destroys', flags, 'Always fails.', () => true, null, null, face(['web'])],
      native: [() => () => {}, {}, 'writes', flags, 'Native only.', null, null, null, face(['apple'])],
      bare: [() => () => {}, {}, 'writes', flags, 'No face.', null, null, null, null],
    });
    await settle();
    expect(browser.registered.map(r => r.tool.name)).toEqual(['waitFor', 'look', 'broken']);
    expect(rt.journal.filter(l => l.startsWith('webmcp'))).toEqual(['webmcp waitFor: registered', 'webmcp look: registered', 'webmcp broken: registered']);

    const [wait] = browser.named('waitFor'), [look] = browser.named('look'), [broken] = browser.named('broken');
    expect(wait.tool).toMatchObject({
      title: 'Wait For', description: 'Wait for ms milliseconds.',
      inputSchema: { type: 'object', properties: { ms: { type: 'integer', title: 'Milliseconds' }, size: { type: 'string', enum: ['small', 'large'], title: 'Size' } },
        required: ['ms', 'size'], additionalProperties: false },
      annotations: { readOnlyHint: false, consequentialHint: true },
    });
    expect(look.tool).toMatchObject({ title: 'look', inputSchema: { properties: { q: { type: 'string' } }, required: ['q'] },
      annotations: { readOnlyHint: true, consequentialHint: false } });
    expect(broken.tool.annotations).toEqual({ readOnlyHint: false, consequentialHint: true });

    expect(await wait.tool.execute({ ms: 250, size: 'small' })).toBe(1);
    expect(waited).toEqual([[250, 'small']]);
    expect(await look.tool.execute({ q: 'x' })).toEqual({ outcome: 'ok' });
    expect(await broken.tool.execute({})).toEqual({ outcome: 'failed', certainty: 'uncertain', reason: '',
      message: 'It may or may not have taken effect: read before calling again.' });
    expect(await wait.tool.execute({ ms: 'soon', size: 'small' })).toEqual({ outcome: 'refused', certainty: 'none', reason: 'arguments',
      message: 'ms: a finite number, not "soon"' });
    expect(globalThis.exact.calls.state().calls.map(c => [c.tool, c.door, c.outcome])).toEqual([
      ['waitFor', 'web', 'ok'], ['look', 'web', 'ok'], ['broken', 'web', 'failed'], ['waitFor', 'web', 'refused']]);

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
