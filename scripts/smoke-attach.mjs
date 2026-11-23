#!/usr/bin/env bun
// Agent attach end to end (LLP 1012.000): a development Shelf opened as a
// person opens it, its Enable Agent Attach, Allow and Allow Agent Input
// pressed for the person (System Events on the Mac, which the terminal needs
// the Accessibility grant for; the touch runner in the iOS simulator), then
// `agent.mjs --attach` reads, taps and waits, and each refusal is sent raw on
// the wire. The app must already be built for the host:
//   bun host/apple/build.mjs shelf                  then  bun scripts/smoke-attach.mjs macos
//   bun host/apple/build.mjs shelf --ios --sim <s>  then  bun scripts/smoke-attach.mjs ios --sim <s>
// It refuses to start while a Shelf is already running there, so it never
// drives or closes one it did not open, and quits the one it opened.
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, readdirSync } from 'node:fs';
import { connect } from 'node:net';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { attachTarget, attachTargetIOS } from './agent-attach.mjs';
import { attachWire } from './agent-wire.mjs';
import { resolveApp } from './app.mjs';
import { chooseSimulator } from '../host/apple/devices.mjs';
import { openTouches } from '../host/apple/touches.mjs';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const argv = process.argv.slice(2);
const host = argv[0];
if (!['macos', 'ios'].includes(host)) {
  console.error('usage: bun scripts/smoke-attach.mjs macos|ios [--sim <udid|name>]');
  process.exit(2);
}
if (argv.includes('--sim')) process.env.EXACT_SIM = argv[argv.indexOf('--sim') + 1];
const app = resolveApp(resolve(ROOT, 'apps/shelf'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failures = 0;
const check = (ok, what) => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) failures += 1;
};

// The person's hand, and the app it opened.
let person, quit;
if (host === 'macos') {
  const bundle = readdirSync(resolve(ROOT, 'target/clients')).map((d) => resolve(ROOT, 'target/clients', d, `${app.id}/macos/Shelf.app`)).find(existsSync);
  if (!bundle) throw new Error('no development Shelf for macOS: bun host/apple/build.mjs shelf');
  if (spawnSync('pgrep', ['-x', 'Shelf']).status === 0) throw new Error('a Shelf is already running on this Mac: quit it first, so the smoke drives only the one it opens');
  spawnSync('open', ['-n', bundle]);
  await sleep(6000);
  const pid = pids(bundle)[0];
  if (!pid) throw new Error(`${bundle} did not start`);
  process.env.SMOKE_ATTACH_PID = String(pid);
  const ui = (script) => spawnSync('osascript', ['-e', `tell application "System Events" to tell process "Shelf" to ${script}`], { encoding: 'utf8' });
  const menu = (item) => ui(`click menu item "${item}" of menu 1 of menu bar item "Develop" of menu bar 1`);
  person = {
    front: () => ui('set frontmost to true'),
    enable: () => menu('Enable Agent Attach'),
    allow: () => ui('click button "Allow" of sheet 1 of window 1'),
    input: () => menu('Allow Agent Input'),
    detach: () => menu('Detach Agent'),
  };
  quit = () => process.kill(pid);
} else {
  const dev = chooseSimulator(undefined, { app: app.id, hint: '--sim <udid|name>' });
  // One runner session per act, one act at a time: the driver's own taps take the runner's lock between them.
  let acts = Promise.resolve();
  const hand = (act) => (acts = acts.catch(() => {}).then(async () => {
    const t = await openTouches({ udid: dev.udid, appId: app.id });
    try { return await act(t, (await t.ask({ op: 'screen' })).displays[0]); } finally { await t.close(); }
  }));
  const tapLabel = (label) => hand(async (t, display) => {
    for (let tries = 0; tries < 10; tries++) {
      const hit = (await t.ask({ op: 'tree' })).elements?.filter((e) => e.label === label && e.enabled).at(-1);
      if (hit) { const [x, y, w, h] = hit.frame; return t.ask({ op: 'springboard', action: 'tap', screen: [x + w / 2, y + h / 2], display }); }
      await sleep(500);
    }
    throw new Error(`no ${label} on the simulator's screen`);
  });
  const menuRow = async (row) => {
    await hand((t, display) => t.ask({ op: 'springboard', action: 'tap', screen: [126, 176, 226, 276].map((x) => [x, 437]), display }));
    await sleep(1500);
    await tapLabel(row);
    await sleep(1500);
  };
  const running = (spawnSync('ps', ['-axo', 'command='], { encoding: 'utf8' }).stdout ?? '').split('\n').some((l) => l.includes(`/Devices/${dev.udid}/`) && /\/Shelf\.app\/Shelf$/.test(l));
  if (running) throw new Error(`Shelf is already running in simulator ${dev.name}: close it first, so the smoke drives only the one it opens`);
  await hand(async (t, display) => {
    const icon = (await t.ask({ op: 'springboard', action: 'find', label: 'Shelf' })).elements.find((e) => e.frame[2] > 0);
    if (!icon) throw new Error('Shelf is not on the simulator\'s Home Screen: bun host/apple/build.mjs shelf --ios --sim <s>');
    await t.ask({ op: 'springboard', action: 'tap', screen: [icon.frame[0] + icon.frame[2] / 2, icon.frame[1] + icon.frame[3] / 2], display });
  });
  await sleep(3000);
  person = { front: () => {}, enable: () => menuRow('Enable Agent Attach'), allow: () => tapLabel('Allow'), input: () => menuRow('Allow Agent Input'), detach: () => menuRow('Detach Agent') };
  quit = () => spawnSync('xcrun', ['simctl', 'terminate', dev.udid, app.id]);
}

/** A raw attached connection, allowed by the person. */
const attach = async () => {
  const m = host === 'ios' ? attachTargetIOS({ id: app.id, name: 'Shelf', app: 'shelf', appDir: app.dir }) : attachTarget({ id: app.id, name: 'Shelf', app: 'shelf', pid: Number(process.env.SMOKE_ATTACH_PID) });
  let asked = false;
  const wire = await attachWire(connect({ path: m.socket }), { nonce: m.nonce, onWaiting: () => { asked = true; sleep(800).then(() => person.allow()).catch(() => {}); } });
  check(asked, 'the app asked the person before it attached');
  return { m, wire };
};
/** `agent.mjs --attach` with `ops`, the person pressing Allow and Allow Agent Input as the driver asks for them. */
const drive = (ops) => new Promise((done) => {
  const child = spawn(process.execPath, [resolve(ROOT, 'scripts/agent.mjs'), host, '--app', app.dir, '--attach', ...(process.env.SMOKE_ATTACH_PID ? [process.env.SMOKE_ATTACH_PID] : []), '--json', ...ops], { cwd: ROOT, env: process.env });
  let out = '', err = '';
  child.stdout.on('data', (d) => { out += d; });
  child.stderr.on('data', async (d) => {
    err += d;
    try {
      if (/waiting for Allow in/.test(String(d))) { await sleep(800); await person.allow(); }
      if (/waiting for Allow Agent Input/.test(String(d))) { await sleep(500); await person.input(); }
    } catch (e) { err += `\nthe person's hand failed: ${e.message}`; }
  });
  child.on('close', (code) => done({ code, out, err, replies: out.split('\n').filter(Boolean).map((l) => { try { return JSON.parse(l); } catch { return {}; } }) }));
});

try {
  person.front();
  if (host === 'macos' && person.enable().status !== 0) throw new Error('System Events could not click Develop ▸ Enable Agent Attach: give this terminal the Accessibility grant');
  if (host === 'ios') await person.enable();
  // Reads, then every refusal sent raw before input is granted.
  const { wire } = await attach();
  check(wire.attached.grants?.input === false, 'allowed for reads, input not granted');
  const tree = await wire.ask({ op: 'tree' });
  check(JSON.stringify(tree).includes('library') || JSON.stringify(tree).includes('book'), 'tree reads the app');
  check(typeof (await wire.ask({ op: 'state' })).observation?.wall === 'number', 'state carries the wall-clock observation');
  const refusals = [
    [{ op: 'tap', target: 'goal-up' }, 'input-not-granted'], [{ op: 'clock', seek: 10 }, 'live-clock'], [{ op: 'state', reveal: true }, 'masked'],
    [{ op: 'tree', session: 'main' }, 'pinned'], [{ op: 'quit' }, 'unsupported'], [{ op: 'prefer', scheme: 'dark' }, 'unsupported'],
    [{ op: 'call', tool: 'addToShelf', door: 'mcp' }, 'door'], [{ op: 'layout', id: 5000000000 }, 'malformed'], [{ op: 'tree', bogus: 1 }, 'malformed'],
  ];
  for (const [req, code] of refusals) check((await wire.ask(req)).refused === code, `${JSON.stringify(req)} refused ${code}`);
  // One driver at a time.
  const m2 = host === 'ios' ? attachTargetIOS({ id: app.id, name: 'Shelf', app: 'shelf', appDir: app.dir }) : attachTarget({ id: app.id, name: 'Shelf', app: 'shelf', pid: Number(process.env.SMOKE_ATTACH_PID) });
  const busy = await attachWire(connect({ path: m2.socket }), { nonce: m2.nonce }).catch((e) => e);
  check(/busy/.test(busy.message ?? ''), 'a second driver is refused busy');
  // Which page is on top, as the app's own tree says, hidden nodes left out.
  const shownTree = JSON.stringify(((await wire.ask({ op: 'tree' })).nodes ?? []).filter((n) => !n.hidden));
  const wire2tree = async () => shownTree;
  wire.close();
  await sleep(1500);

  // Through the driver: a granted tap and its effect, a wait on the wall clock, and a clock control refused.
  // Shelf restores the page it last showed. On the Mac a book row opens the book beside the library; the iPhone's
  // library filters, as the Mac's toolbar does, and a book's page sets its status.
  const library = (await wire2tree()).includes('"library-list"');
  const [target, until] = host === 'macos' ? ['book-seed-2', 'current.name == "book"']
    : library ? ['filter-done', 'filter == "done"'] : ['detail-status-reading', 'current.name == "book"'];
  const tapped = await drive([`tap ${target}`, `clock until state ${until} within 3000`, 'clock replies 5s']);
  check(tapped.code === 0, `the driver tapped and waited (exit ${tapped.code})${tapped.code ? `: ${tapped.err.split('\n').filter((l) => /^op |error:/.test(l)).join(' ')}` : ''}`);
  check(tapped.replies.some((r) => r.delivery === 'platform'), 'the tap was a platform delivery');
  check(tapped.replies.some((r) => r.until === 'met'), 'clock until met on the app\'s own clock');
  const clock = await drive(['clock +100']);
  check(clock.code !== 0 && /live-clock/.test(clock.err), 'clock +100 is refused live-clock');

  // Revocation and Detach.
  const { wire: again } = await attach();
  person.front();
  await person.input();
  check((await again.ask({ op: 'grants' })).input === true, 'Allow Agent Input grants input');
  await person.input();
  check((await again.ask(host === 'macos' ? { op: 'tap', target } : { op: 'tap', target, aim: true })).refused === 'input-not-granted', 'revoked input refuses a tap, reads go on');
  check(!(await again.ask({ op: 'tags' })).error, 'a read after the revocation');
  await person.detach();
  await sleep(1000);
  check(again.failure != null, 'Detach Agent ends the connection');
  const gone = await drive(['tree']);
  check(/has not turned Agent Attach on/.test(gone.err), 'after Detach the driver says attach is off');
} finally {
  quit();
}
console.log(failures ? `${failures} failed` : 'all passed');
process.exit(failures ? 1 : 0);

function pids(bundle) {
  return (spawnSync('pgrep', ['-f', `${bundle}/Contents/MacOS/`], { encoding: 'utf8' }).stdout ?? '').split('\n').filter(Boolean).map(Number);
}
