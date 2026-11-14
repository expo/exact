import { hostFixtureFile as readFileSync } from './glue-fixture.mjs';
// LLP 1166.000 D2 in a real browser: the cases the kernel's `role_step` and `role_tab_stop` answer
// (kernel/tables/role-key-cases.json, which kernel/tests/it/role_keys.rs runs) put to the web's own
// implementation, the input piece both web targets load (input-glue.js). Each case's tree is plain DOM under the
// root the piece watches, as radio-group.test.mjs mounts its group, and the keys are real ones (CDP key events), so
// the browser's own focus rules and Tab order apply.
// A node's `visibility`, like its `direction`, is its inline style.
import { test, expect } from 'bun:test';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { cdpBrowser, chromium } from '../../../scripts/agent-launch.mjs';
import { withLook } from '../look.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { readyPage } from './page.mjs';
import { fileURLToPath } from 'node:url';

const WEB = resolve(fileURLToPath(new URL('..', import.meta.url)));
const { cases } = JSON.parse(readFileSync(new URL('../../../kernel/tables/role-key-cases.json', import.meta.url), 'utf8'));
const { unavailable } = chromium();
const check = need('chrome', unavailable);

const page = withLook(readFileSync(resolve(WEB, 'index.html'), 'utf8')).match(/<style>[\s\S]*?<\/style>/)[0] + `
<div id="exact-root"></div>
<script type="module">
  import { createInputHandlers } from './input-glue.js';
  const root = document.getElementById('exact-root');
  // What a key did: each press in the root (recorded before any listener; a link's navigation is held back), and
  // whether a listener consumed the key (read after them all).
  window.pressed = [];
  addEventListener('click', e => { if (root.contains(e.target)) { pressed.push(e.target.id); e.preventDefault(); } }, true);
  addEventListener('keydown', e => { window.consumed = e.defaultPrevented; });
  // A case's node as the hosts write one: its tag, \`role\`, literal attributes and \`direction\`. \`hidden\` is
  // \`display: none\` (the cases' \`_about\`), and a boolean attribute is present when true. A leaf's text is its id.
  const build = ({ id, tag = 'div', role, attrs = {}, direction, visibility, children = [] }) => {
    const el = document.createElement(tag);
    el.id = id;
    if (role) el.setAttribute('role', role);
    for (const [name, value] of Object.entries(attrs)) {
      if (name === 'hidden') { if (value) el.style.display = 'none'; }
      else if (name === 'disabled' || name === 'inert') el.toggleAttribute(name, value === true);
      else el.setAttribute(name, String(value));
    }
    if (direction) el.style.direction = direction;
    if (visibility) el.style.visibility = visibility;
    if (children.length) el.append(...children.map(build)); else el.textContent = id;
    return el;
  };
  // A button before the group and one after it, to Tab from. When the case's \`axis\` is vertical, the group's parent
  // has the mark the navigation glue gives a root tab bar's parent when it draws the bar as a sidebar.
  window.mount = (tree, sidebar) => {
    const holder = document.createElement('div'), button = id => Object.assign(document.createElement('button'), { id, textContent: id });
    holder.toggleAttribute('data-exact-sidebar', sidebar);
    holder.append(build(tree));
    root.replaceChildren(button('before'), holder, button('after'));
    pressed.length = 0;
  };
  createInputHandlers({ root, views: new Map(), retiredViews: new Set(), ready: () => true, inertAncestor: el => el.closest('[inert]'), dispatch() {} });
  window.ready = true;
</script>`;

/** The page in headless Chrome: `test(evaluate, key)`, where `key(name, shift)` presses a real key, Shift held when `shift`. */
async function inChrome(test) {
  const server = createServer((req, res) => {
    if (req.url === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(page); return; }
    const file = ['navigation.js', 'input-glue.js', 'prompt-glue.js', 'alert-message.js'].find(name => req.url === '/' + name);
    if (!file) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'content-type': 'text/javascript' }); res.end(readFileSync(resolve(WEB, file)));
  });
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
  const suite = await cdpBrowser({ args: ['--window-size=600,400'] }), { cdp } = suite;
  try {
    const { sessionId } = await suite.page();
    const call = (method, params) => cdp.send(method, params, sessionId);
    const evaluate = async (expression) => {
      const reply = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
      if (reply.exceptionDetails) throw new Error(reply.exceptionDetails.exception?.description ?? reply.exceptionDetails.text);
      return reply.result.value;
    };
    await readyPage(cdp, sessionId, `http://127.0.0.1:${server.address().port}/`);
    const key = async (name, shift = false) => {
      for (const type of ['keyDown', 'keyUp']) await call('Input.dispatchKeyEvent', { type, key: name, code: name, windowsVirtualKeyCode: CODES[name], modifiers: shift ? 8 : 0 });
    };
    await test(evaluate, key);
  } finally {
    await suite.close();
    server.close();
  }
}

const CODES = { Tab: 9, End: 35, Home: 36, ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40 };
const ids = node => [node.id, ...(node.children ?? []).flatMap(ids)];
const nodes = node => [node, ...(node.children ?? []).flatMap(nodes)];
// The attributes kernel/tests/it/role_keys.rs maps; it refuses any other, and so does this.
const ATTRIBUTES = ['disabled', 'aria-disabled', 'aria-checked', 'aria-selected', 'aria-orientation', 'tabindex', 'inert', 'href', 'hidden'];
const mount = c => {
  if (c.axis !== undefined && c.axis !== 'vertical') throw new Error(`${c.name}: unknown orientation override ${c.axis}`);
  const unknown = nodes(c.tree).flatMap(n => Object.keys(n.attrs ?? {})).filter(name => !ATTRIBUTES.includes(name));
  if (unknown.length) throw new Error(`${c.name}: unknown case attribute ${unknown.join(', ')}`);
  return `mount(${JSON.stringify(c.tree)}, ${c.axis === 'vertical'})`;
};
/** Focuses the case's member, refusing a case whose member the browser will not focus. */
async function focus(evaluate, c) {
  const at = await evaluate(`${mount(c)}; document.getElementById(${JSON.stringify(c.focus)}).focus(); document.activeElement.id`);
  if (at !== c.focus) throw new Error(`${c.name}: ${c.focus} took no focus (${at} has it)`);
}

// A key case: the member the focus lands on and whether it is pressed, or `null` for a key that is not the group's,
// which moves nothing and is left to the page (not consumed). A destination that is the focused member is a key the
// group consumes without moving.
check('each key case moves the focus and presses as the kernel answers', () => inChrome(async (evaluate, key) => {
  const differ = [], keyed = cases.filter(c => c.key !== undefined);
  expect(keyed.length).toBeGreaterThan(0);
  for (const c of keyed) {
    await focus(evaluate, c);
    await evaluate('window.consumed = null');
    await key(c.key);
    const got = await evaluate('({ focus: document.activeElement.id, pressed: pressed.splice(0), consumed })');
    const { target, press } = c.expected;
    const want = target === null ? { focus: c.focus, pressed: [], consumed: false } : { focus: target, pressed: press ? [target] : [], consumed: true };
    if (!Bun.deepEquals(got, want)) differ.push(`${c.name}: got ${JSON.stringify(got)}, expected ${JSON.stringify(want)}`);
  }
  expect(differ).toEqual([]);
}), 120000);

// A Tab-stop case, as a person finds the stop: with nothing in the group focused, Tab from the button before it and
// Shift-Tab from the one after it each stop at the case's member and then leave, or pass the group when it has none.
// With a member focused, the stop is the member the piece leaves in the Tab order (a `tabIndex` of 0 or more) as Tab
// goes down, and the focus leaves the group. A menu sets no stop (D1), which on the web leaves each item its own: Tab
// reaches every one in turn.
check('each Tab-stop case stops where the kernel answers', () => inChrome(async (evaluate, key) => {
  const differ = [], stopped = cases.filter(c => c.key === undefined);
  expect(stopped.length).toBeGreaterThan(0);
  /** The members a walk of Tab (or Shift-Tab) from `from` to `to` focuses; any other place it reaches ends it. */
  const walk = async (members, from, to, shift) => {
    const seen = [];
    await evaluate(`document.getElementById('${from}').focus()`);
    for (let i = 0; i <= members.length; i++) {
      await key('Tab', shift);
      const at = await evaluate('document.activeElement.id');
      if (!members.includes(at)) return at === to ? seen : [...seen, `(${at || 'body'})`];
      seen.push(at);
    }
    return [...seen, '(no exit)'];
  };
  for (const c of stopped) {
    const members = ids(c.tree).slice(1), menu = c.tree.role === 'menu';
    if (menu && c.stop !== null) throw new Error(`${c.name}: a menu sets no stop`);
    let got, want;
    if (c.focus === null) {
      await evaluate(mount(c));
      const enter = menu ? members : c.stop === null ? [] : [c.stop];
      got = { tab: await walk(members, 'before', 'after', false), shiftTab: await walk(members, 'after', 'before', true) };
      want = { tab: enter, shiftTab: [...enter].reverse() };
    } else {
      await focus(evaluate, c);
      await key('Tab');
      got = await evaluate(`({ stops: ${JSON.stringify(members)}.filter(id => document.getElementById(id).tabIndex >= 0), focus: document.activeElement.id })`);
      want = menu ? { stops: members, focus: members[members.indexOf(c.focus) + 1] ?? 'after' } : { stops: c.stop === null ? [] : [c.stop], focus: 'after' };
    }
    if (!Bun.deepEquals(got, want)) differ.push(`${c.name}: got ${JSON.stringify(got)}, expected ${JSON.stringify(want)}`);
  }
  expect(differ).toEqual([]);
}), 120000);
