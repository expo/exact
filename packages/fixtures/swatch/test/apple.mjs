// @fixture/swatch on Apple (LLP 1133.001 D1, LLP 1133.008 D8 items 1 and 2):
// the fixture app made outside the repo, built for macOS, an iOS simulator
// or the sample host (`--host`, two sessions in one process), and driven by
// the agent. The package's view is made by its session's instance of the
// implementation, which hears the view's displayed state and its `tint`
// binding change with the origin of the commit that made each (`app` here:
// the agent's taps are a person's input; a tool's origin is the runner's,
// tested in contract/cli/tests/it/module_views.rs, since no Apple host has a
// tool door yet), and answers what it heard, with what it read of its
// configuration (`ExactPackageContext.config`), as `swatchReports`. The key the
// configuration carries is in no build output, tree, state or log, and a
// production build without it is refused `swatch-key-missing`. On iOS each
// view is a view controller's view (`ExactPackageView.controller`), which the
// host contains. The box is in its route's controller. A second view is made
// in the batch that pushes the route it fills, and goes to that route's
// controller once it loads. A third, in that route's dialog, is in the
// sheet's controller while the dialog shows and in the route's once it
// closes, with UIKit's animations on (`timing: 'platform'`).
//
// usage: bun packages/fixtures/swatch/test/apple.mjs [macos] [ios] [host] [--sim <name|udid>]
import { rmSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { LEAK, checker, makeFixture, reportedConfig, root, run } from './fixture.mjs';

const argv = process.argv.slice(2), sim = argv.includes('--sim') ? argv[argv.indexOf('--sim') + 1] : process.env.EXACT_SIM;
const hosts = argv.filter((a, i) => ['macos', 'ios', 'host'].includes(a) && argv[i - 1] !== '--sim');
const { app } = makeFixture(), name = basename(app);
Object.assign(process.env, { EXACT_APP_DIR: app, EXACT: root, HERMES_LEAN_SYS_OFFLINE: '1' });
const { open } = await import(resolve(root, 'scripts/agent.mjs'));
const { check, failed } = checker();

/** Each testId's text in the tree. */
const texts = async (s) => {
  const { nodes } = await s.tree(), byId = new Map(nodes.map((n) => [n.id, n]));
  const text = (n) => n.props?.text ?? n.children.map((c) => text(byId.get(c))).join('');
  return Object.fromEntries(nodes.filter((n) => n.props?.testId).map((n) => [n.props.testId, text(n)]));
};
/** `heard` as its lines once `want` holds of them, or the last seen. */
const heard = async (s, want, ms = 10000) => {
  const end = Date.now() + ms;
  for (;;) {
    await s.clock('settle');
    const lines = ((await texts(s)).heard ?? '').split('; ').filter(Boolean);
    if (want(lines) || Date.now() >= end) return lines;
    await s.clock('+100 real');
  }
};
const count = (lines, line) => lines.filter((l) => l === line).length;

/** One session's drive: shown, rebound, hidden and shown, tapped, removed. */
async function drive(s, where) {
  const target = where === 'ios-simulator' ? 'ios' : 'macos';
  let config = '';
  for (const end = Date.now() + 10000; Date.now() < end && !config; await s.clock('+100 real')) config = (await texts(s)).config;
  check(where, "the package's Swift reads its configuration", config === reportedConfig(target), config);
  const none = await heard(s, () => true, 0);
  check(where, 'nothing is reported before the view is placed', none.length === 0, none.join('; '));
  await s.tap('show');
  const shown = await heard(s, (l) => l.some((x) => x.startsWith('made ')) && l.some((x) => x.startsWith('displayed ')) && l.some((x) => x.startsWith('bound ')) &&
    (where !== 'ios-simulator' || l.some((x) => x.startsWith('held '))));
  const id = shown.find((x) => x.startsWith('made '))?.split(' ')[1];
  check(where, "the session's instance made the view", Boolean(id), shown.join('; '));
  const box = (await s.tree()).nodes.find((n) => n.props?.testId === 'box');
  check(where, "the view's id is its node's", String(box?.id) === id, `${box?.id} beside made ${id}`);
  check(where, "shown by the app's commit: displayed true app", shown.includes(`displayed ${id} true app`), shown.join('; '));
  check(where, 'its binding prop reported with its origin', shown.includes(`bound ${id} tint app`), shown.join('; '));
  const state = await s.state();
  const views = state.extensions?.swatch?.views ?? [];
  check(where, 'state.extensions names the view and who showed it', views.length === 1 && views[0].displayed === true && views[0].origin === 'app', JSON.stringify(state.extensions));

  await s.tap('recolor');
  const rebound = await heard(s, (l) => count(l, `bound ${id} tint app`) === 2);
  check(where, 'a rebinding is reported again', count(rebound, `bound ${id} tint app`) === 2, rebound.join('; '));
  await s.tap('hide');
  const hidden = await heard(s, (l) => l.includes(`displayed ${id} false app`));
  check(where, 'display: none is reported hidden', hidden.includes(`displayed ${id} false app`), hidden.join('; '));
  await s.tap('hide');
  const again = await heard(s, (l) => count(l, `displayed ${id} true app`) === 2);
  check(where, 'shown again is reported again', count(again, `displayed ${id} true app`) === 2, again.join('; '));

  const tapped = await s.tap('box').catch((e) => ({ error: e.message }));
  await s.clock('settle');
  const last = (await texts(s)).last;
  check(where, "the view's event reaches its handler", last === 'tap', `${last} after ${JSON.stringify(tapped)}`);

  await s.tap('show');
  const removed = await heard(s, (l) => count(l, `displayed ${id} false app`) === 2);
  check(where, 'a shown view leaving the tree is reported hidden', count(removed, `displayed ${id} false app`) === 2, removed.join('; '));
  if (where === 'ios-simulator') check(where, "the box's controller is held by a controller whose view holds the box", shown.includes(`held ${id} true`), shown.join('; '));
  const said = JSON.stringify([await s.tree(), await s.state(), await s.logs({ all: true })]);
  check(where, 'the key is in no tree, state or log', !said.includes(LEAK), "the key is in the drive's output");
  return id;
}

/** Views made with the route one fills, pushed; the route's dialog shown
 * and closed; the route popped. */
async function screen(s, where) {
  const ios = where === 'ios-simulator';
  const before = await heard(s, () => true, 0);
  await s.tap('open');
  await s.clock('settle');
  const node = async (testId) => String((await s.tree()).nodes.find((n) => n.props?.testId === testId)?.id);
  const id = await node('screen-box'), sheet = await node('sheet-box');
  const pushed = await heard(s, (l) => l.includes(`displayed ${id} true app`) && (!ios || l.includes(`held ${id} true`)));
  check(where, 'the route and its dialog make their views', !before.includes(`made ${id}`) && pushed.includes(`made ${id}`) && pushed.includes(`made ${sheet}`), pushed.join('; '));
  check(where, "the route's view is shown", pushed.includes(`displayed ${id} true app`), pushed.join('; '));
  const holders = async () => [...JSON.stringify(await s.logs({ all: true })).matchAll(new RegExp(`#${sheet}: a screen in (\\w+)`, 'g'))].map((m) => m[1]);
  if (ios) {
    check(where, "its controller is held by a controller whose view holds it", pushed.includes(`held ${id} true`), pushed.join('; '));
    const logs = JSON.stringify(await s.logs({ all: true }));
    check(where, "the route's controller holds it", logs.includes(`#${id}: a screen in RouteController`), logs.slice(-1500));
    check(where, "the dialog's view starts in the route's controller", (await holders()).join() === 'RouteController', (await holders()).join());
  }
  /** The host's moves of the dialog's view once there are `n`, or the last seen. */
  const moves = async (n) => {
    for (const end = Date.now() + 10000; ; await s.clock('+100 real')) {
      await s.clock('settle');
      const now = await holders();
      if (now.length >= n || Date.now() >= end) return now;
    }
  };
  const consistent = async () => !(await heard(s, () => true, 0)).some((x) => x.startsWith('held ') && x.endsWith(' false'));
  await s.tap('open-sheet');
  if (ios) {
    const open = await moves(2);
    check(where, "the dialog's view moves into the sheet's controller", open.join() === 'RouteController,DialogSheetController' && await consistent(), `${open.join()} / ${(await heard(s, () => true, 0)).join('; ')}`);
  }
  await s.tap('close-sheet');
  if (ios) {
    const closed = await moves(3);
    check(where, "the closed dialog's view moves back to the route's controller", closed.join() === 'RouteController,DialogSheetController,RouteController' && await consistent(), `${closed.join()} / ${(await heard(s, () => true, 0)).join('; ')}`);
  }
  await s.back();
  const popped = await heard(s, (l) => l.includes(`displayed ${id} false app`));
  check(where, 'a popped route is reported hidden', popped.includes(`displayed ${id} false app`), popped.join('; '));
}

for (const host of hosts.length ? hosts : ['macos', 'ios', 'host']) {
  const where = host === 'ios' ? 'ios-simulator' : host === 'host' ? 'sample-host' : 'macos';
  const platform = host === 'ios' ? ['--ios', ...(sim ? ['--sim', sim] : [])] : host === 'host' ? ['--host'] : [];
  const said = run('bun', [resolve(root, 'host/apple/build.mjs'), ...platform, `${name}-apple`], root);
  check(where, 'the build says nothing of the key', !said.includes(LEAK), 'the key is in the build output');
  if (host !== 'host') {
    // A production build without the variable is refused before any package compiles.
    const refused = spawnSync('bun', [resolve(root, 'host/apple/build.mjs'), ...platform, '--release', `${name}-apple`], { cwd: root, env: { ...process.env, SWATCH_KEY: '' }, encoding: 'utf8', maxBuffer: 1 << 28 });
    check(where, 'without the variable a production build is refused swatch-key-missing', refused.status !== 0 && refused.stderr.includes('swatch-key-missing'), (refused.stdout + refused.stderr).slice(-1500));
  }
  if (host !== 'host') {
    // A view's recognizers take only a real touch on iOS (`--touch platform`).
    const s = await open({ host, app: name, ...(host === 'ios' ? { touch: 'platform' } : {}) });
    try { await drive(s, where); if (host !== 'ios') await screen(s, where); } finally { await s.close(); }
    if (host === 'ios') {
      // UIKit's animations on: a sheet's dismissal ends after its view has left the window.
      const animated = await open({ host, app: name, touch: 'platform', timing: 'platform' });
      try { await screen(animated, where); } finally { await animated.close(); }
    }
    continue;
  }
  // Two sessions of the one plan in one process, each its own instance: a's
  // reports never reach b's, though their node ids are the same.
  const s = await open({ host: 'host', app: name, session: 'a' });
  try {
    check(where, 'the host runs sessions a and b', s.sessions?.join(',') === 'a,b', JSON.stringify(s.sessions));
    const a = await drive(s, `${where} a`);
    s.session = 'b';
    const quiet = await heard(s, () => true, 0);
    check(where, "b's instance heard nothing of a's view", quiet.length === 0, quiet.join('; '));
    await s.tap('show');
    const b = await heard(s, (l) => l.some((x) => x.startsWith('displayed ')));
    const id = b.find((x) => x.startsWith('made '))?.split(' ')[1];
    check(where, "b's view reached b's own instance", b.includes(`displayed ${id} true app`) && b.filter((x) => x.startsWith('made ')).length === 1, b.join('; '));
    check(where, 'the two sessions share node ids, not instances', id === a, `a ${a}, b ${id}`);
    s.session = 'a';
    const after = await heard(s, () => true, 0);
    check(where, "a's instance heard nothing of b's view", after.filter((x) => x.startsWith('made ')).length === 1 && after.every((x) => !x.endsWith('true app') || count(after, x) === 2), after.join('; '));
  } finally {
    await s.close();
  }
}
rmSync(dirname(app), { recursive: true, force: true });
console.log(failed() ? `${failed()} failed` : 'all passed');
process.exit(failed() ? 1 : 0);
