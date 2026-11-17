// The conformance drive of @exact/push (LLP 1133.004 §9): the fixture app
// (`fixture/`), made by `exact new` outside the repo and depending on this
// package by `file:`, on an iOS simulator, and its absence on the web.
//
// - Under the agent: `subscribePush()` waits on the notifications permission
//   ticket, then on the subscription's value ticket; the value typed there
//   is the answer and the live value; a value preferred later is a rotation.
// - Bound (`--bound`): the system's prompt, allowed through SpringBoard,
//   then the system's answer to the registration: the call answers a
//   subscription and the live value shows its token, or the call fails with
//   the system's own failure (never the package's 60 s bound); then a
//   relaunch that only reads `pushSubscription()` answers the same token.
//
// - On this Mac (`word.swift`, `service-reference.swift`): the branch `subscribePush()` takes for each
//   notification authorization, from the function it switches on, and the
//   state `pushPermissionState()` reads: only `notDetermined` asks;
//   provisional and ephemeral register without asking, as authorized does.
//
// usage: bun packages/push/test/drive.mjs [--bound] [--sim <udid>] [--keep]
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const here = import.meta.dir, pkg = resolve(here, '..'), root = resolve(pkg, '../..');
const argv = process.argv.slice(2), sim = argv.includes('--sim') ? argv[argv.indexOf('--sim') + 1] : null;
const run = (cmd, args, cwd, env = process.env) => {
  const r = spawnSync(cmd, args, { cwd, env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')}: ${r.stdout}\n${r.stderr}`);
  return r.stdout;
};
let failures = 0;
const check = (name, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'} ios ${name}${ok ? '' : `: ${detail}`}`);
};

// The branch subscribePush() takes for each authorization, compiled with Push.swift and run on this Mac (word.swift).
{
  const out = mkdtempSync(join(tmpdir(), 'exact-push-word-'));
  run('swiftc', ['-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage', '-emit-module', '-emit-module-path', join(out, 'ExactPackage.swiftmodule'),
    resolve(root, 'host/apple/modules/ExactPackage.swift'), '-o', join(out, 'libExactPackage.a')], out);
  run('swiftc', ['-parse-as-library', '-I', out, '-L', out, '-lExactPackage', resolve(pkg, 'apple/Push.swift'), resolve(here, 'word.swift'), '-o', join(out, 'word')], out);
  const r = spawnSync(join(out, 'word'), [], { encoding: 'utf8' });
  for (const line of r.stdout.trim().split('\n')) check(line.replace(/^(PASS|FAIL) /, ''), line.startsWith('PASS'), line);
  if (r.status !== 0 && !r.stdout.includes('FAIL')) check('word.swift ran', false, r.stderr);
  rmSync(out, { recursive: true, force: true });
}

// The service handler's completion and fallback cases run directly, distinct from a system invocation below.
{
  const out = mkdtempSync(join(tmpdir(), 'exact-push-service-'));
  const service = resolve(here, 'fixture/modules/apple/notification-service/NotificationService.swift');
  const r = spawnSync('swiftc', [service, resolve(here, 'service-reference.swift'), '-o', join(out, 'reference')], { encoding: 'utf8' });
  if (r.status === 0) {
    const reference = spawnSync(join(out, 'reference'), [], { encoding: 'utf8' });
    for (const line of reference.stdout.trim().split('\n')) check(line.replace(/^(PASS|FAIL) /, ''), line.startsWith('PASS'), line);
    if (reference.status !== 0 && !reference.stdout.includes('FAIL')) check('notification service reference ran', false, reference.stderr);
  } else check('notification service reference compiled', false, r.stderr);
  rmSync(out, { recursive: true, force: true });
}

const scratch = mkdtempSync(join(tmpdir(), 'exact-push-')), app = join(scratch, 'pushfixture');
run('bun', [resolve(root, 'scripts/exact.mjs'), 'new', app], root);
for (const f of ['app.contract', 'app.ts', 'strings', 'modules']) cpSync(join(here, 'fixture', f), join(app, f), { recursive: true });
rmSync(join(app, 'app.test.contract'), { force: true });
const appManifest = JSON.parse(readFileSync(join(app, 'app.json'), 'utf8')), id = appManifest.app.id;
appManifest.share_target = { action: '/', method: 'GET', params: { text: 'text' } };
writeFileSync(join(app, 'app.json'), JSON.stringify(appManifest, null, 2));
writeFileSync(join(app, 'app.ts'), readFileSync(join(app, 'app.ts'), 'utf8').replace('__APP_ID__', id));
const serviceSource = join(app, 'modules/apple/notification-service/NotificationService.swift');
writeFileSync(serviceSource, readFileSync(serviceSource, 'utf8').replaceAll('__APP_ID__', id));
const manifest = JSON.parse(readFileSync(join(app, 'package.json'), 'utf8'));
manifest.dependencies = { ...manifest.dependencies, '@exact/push': `file:${pkg}` };
writeFileSync(join(app, 'package.json'), JSON.stringify(manifest, null, 2));
run('bun', ['install'], app);

const flags = [...(sim ? ['--sim', sim] : []), '--storage', 'push', '--reset-storage'];
const agent = (...ops) => JSON.parse(`[${run('bun', ['exact.mjs', 'agent', 'ios', '--json', ...flags, ...ops], app).trim().split('\n').filter((l) => l.startsWith('{')).join(',')}]`);
const text = (tree, testId) => tree?.nodes?.find((n) => n.props?.testId === testId)?.props?.text;

{
  // The web has no implementation: its declared absence, not an error, whatever the permission.
  const onWeb = (...ops) => JSON.parse(`[${run('bun', ['exact.mjs', 'agent', 'web', '--json', ...ops], app).trim().split('\n').filter((l) => l.startsWith('{')).join(',')}]`);
  for (const state of ['prompt', 'denied', 'granted']) {
    const [web] = onWeb('--permission', `notifications=${state}`, 'state');
    check(`on the web, ${state}, pushSubscription() reads none and pushPermissionState() its declared absence`,
      web.resources?.subscription === null && !web.failed?.subscription && JSON.stringify(web.resources?.permission) === '{"available":false,"state":"unknown"}', JSON.stringify([web.resources, web.failed]));
  }
  const [empty] = agent('state');
  check('before any value, pushSubscription() reads none, not an error', empty.resources?.subscription === null && !empty.failed?.subscription, JSON.stringify([empty.resources?.subscription, empty.failed]));
  const [, held] = agent('tap subscribe', 'state');
  const permission = (held.pending ?? []).find((p) => p.device?.capability === 'permission')?.ticket;
  check('subscribePush while prompt waits on the notifications permission ticket', permission != null, JSON.stringify(held.pending));
  const allowed = agent('tap subscribe', `tap @${permission} allow`, 'state').at(-1);
  const value = (allowed.pending ?? []).find((p) => p.device?.capability === 'subscription')?.ticket;
  check('allowed, it waits on the subscription\'s value', value != null && allowed.extensions?.push?.permissions?.notifications === 'granted', JSON.stringify(allowed.pending));
  const answered = agent('tap subscribe', `tap @${permission} allow`, `type @${value} {"apple":{"deviceToken":"ab12"}}`, 'clock settle', 'tree',
    'prefer extension push.pushSubscription={"apple":{"deviceToken":"cd34"}}', 'clock settle', 'tree');
  const [first, rotated] = answered.filter((r) => Array.isArray(r.nodes));
  check('the value answers the call and is the live value', text(first, 'subscribed') === 'answered' && text(first, 'token') === 'ab12', JSON.stringify([text(first, 'subscribed'), text(first, 'token')]));
  check('a value preferred later is a rotation the source reads', text(rotated, 'token') === 'cd34', text(rotated, 'token'));
  // While permission is not granted the subscription reads none, a token given before included; granted, the token.
  const turned = agent('--permission', 'notifications=denied', 'prefer extension push.pushSubscription={"apple":{"deviceToken":"ef56"}}', 'clock settle', 'state', 'tree',
    'prefer permission.notifications granted', 'clock settle', 'tree');
  const [deniedState] = turned.filter((r) => r.slots);
  const [denied, granted] = turned.filter((r) => Array.isArray(r.nodes));
  check('denied, pushSubscription() reads none, not NotAllowedError', text(denied, 'token') === 'none' && !deniedState.failed?.subscription, JSON.stringify([text(denied, 'token'), deniedState.failed]));
  check('then granted, it reads the token', text(granted, 'token') === 'ef56', text(granted, 'token'));
}

if (argv.includes('--bound')) {
  const { open } = await import(resolve(root, 'scripts/agent.mjs'));
  process.env.EXACT_APP_DIR = app;
  if (sim) process.env.EXACT_SIM = sim;
  run('bun', ['exact.mjs', 'ios', ...(sim ? ['--sim', sim] : [])], app);
  const flat = (n) => [n, ...(n.children ?? []).flatMap(flat)];
  const read = async (s) => { await s.clock('settle'); return text(await s.tree(), 'token'); };
  let token = null;
  let s = await open({ host: 'ios', touch: 'drag', bind: 'push' });
  try {
    await s.tap('subscribe');
    let allow = null;
    for (const end = Date.now() + 10000; !allow && Date.now() < end; await Bun.sleep(250)) {
      allow = flat((await s.springboard({ action: 'tree' })).tree).find((n) => n.label === 'Allow' && n.type === 9);
    }
    // Already granted on this simulator, the call asks nothing.
    if (allow) { await Bun.sleep(800); await s.springboard({ action: 'tap', point: [allow.frame[0] + allow.frame[2] / 2, allow.frame[1] + allow.frame[3] / 2] }); }
    for (let i = 0; i < 20 && (token = await read(s)) === 'none'; i++) await Bun.sleep(1000);
    const state = await s.state();
    const answered = text(await s.tree(), 'subscribed');
    const journal = (await s.logs({ all: true })).lines.join('\n');
    const failed = state.failed?.subscribed ?? '';
    if (/^[0-9a-f]+$/.test(token ?? '')) {
      check(`bound, the call answers the system's token (${token.length / 2} bytes) and the live value shows it`, answered === 'answered' && state.slots?.subscribed?.apple?.deviceToken === token, JSON.stringify([answered, state.slots?.subscribed]));
    } else {
      // A failure the system forwarded carries its own message; the package's bound is no answer at all.
      const bounded = /no answer to the registration/.test(journal + failed);
      check('bound, the system answered the registration', !bounded && /AbortError/.test(journal + failed), failed || journal.split('\n').slice(-6).join('\n'));
      token = null;
    }
  } finally { await s.close?.(); }
  if (!token) console.log('NOT RUN ios a relaunch that only reads pushSubscription(): the registration gave no token');
  else {
    s = await open({ host: 'ios', touch: 'drag', bind: 'push' });
    try {
      let again = 'none';
      for (let i = 0; i < 20 && (again = await read(s)) === 'none'; i++) await Bun.sleep(1000);
      check('a relaunch that only reads pushSubscription() answers the current token', again === token, again);
    } finally { await s.close?.(); }
  }
}

if (!argv.includes('--keep')) rmSync(scratch, { recursive: true, force: true });
else console.log(`kept ${app}`);
console.log(failures ? `${failures} failed` : 'all passed');
process.exit(failures ? 1 : 0);
