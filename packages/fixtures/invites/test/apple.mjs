// @fixture/invites on Apple (LLP 1133.001.000 §6 piece 5): a package under its
// own scope, installed by `file:` into an app `exact new` made outside the
// repo, asks the host for what only the host owns. Without its grant lines the
// compiler names each (`grant-missing`); no build writes CKSharingSupported or
// ExactHostEvents, since `device.cloudkit` is no row yet and so no build takes
// `cloudkit-share`; a SharePlay line signs Group Activities into the simulator
// build. Driven by the agent, a delivery made through the package reaches the
// session's instance through the host's mailbox. That one made before the
// module loads is drained once after is HostEventsTests' (ExactKit), since
// only the system delivers one then.
//
// usage: bun packages/fixtures/invites/test/apple.mjs [macos] [ios] [--sim <name|udid>]
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const here = import.meta.dir, pkg = resolve(here, '..'), root = resolve(pkg, '../../..');
const argv = process.argv.slice(2), sim = argv.includes('--sim') ? argv[argv.indexOf('--sim') + 1] : process.env.EXACT_SIM;
const hosts = argv.filter((a, i) => ['macos', 'ios'].includes(a) && argv[i - 1] !== '--sim');

const run = (cmd, args, cwd, ok = [0]) => {
  const r = spawnSync(cmd, args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  if (!ok.includes(r.status)) throw new Error(`${cmd} ${args.join(' ')}: ${r.stdout}\n${r.stderr}`);
  return r;
};
let failures = 0;
const check = (where, name, ok, detail) => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'} ${where} ${name}${ok ? '' : `: ${detail}`}`);
};

const dir = mkdtempSync(join(tmpdir(), 'exact-invites-')), app = join(dir, 'invitesfixture');
run('bun', [resolve(root, 'scripts/exact.mjs'), 'new', app], root);
for (const f of ['app.contract', 'app.ts']) cpSync(join(here, 'fixture', f), join(app, f));
rmSync(join(app, 'app.test.contract'), { force: true });
const grant = (lines) => writeFileSync(join(app, 'app.ts'), readFileSync(join(here, 'fixture/app.ts'), 'utf8').replace('__GRANTS__', lines));
grant('');
const manifest = JSON.parse(readFileSync(join(app, 'package.json'), 'utf8'));
manifest.dependencies = { ...manifest.dependencies, '@fixture/invites': `file:${pkg}` };
writeFileSync(join(app, 'package.json'), JSON.stringify(manifest, null, 2));
run('bun', ['install'], app);
Object.assign(process.env, { EXACT_APP_DIR: app, EXACT: root, HERMES_LEAN_SYS_OFFLINE: '1' });

// Each name that needs a grant of its own is named when its line is missing.
const built = run('bun', ['exact.mjs', 'contract', 'build', 'app.contract', '--json'], app, [0, 1]);
const said = `${built.stdout}\n${built.stderr}`;
for (const line of ['device.cloudkit', 'device.shareplay']) {
  check('compiler', `grant-missing names ${line}`, new RegExp(`grant-missing[^\\n]*${line.replace('.', '\\.')}|${line.replace('.', '\\.')}[^\\n]*grant-missing`).test(said), said.slice(0, 2000));
}

const plist = (bundle, mac) => JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', join(bundle, mac ? 'Contents/Info.plist' : 'Info.plist')], app).stdout);
// A simulator build carries its entitlements in the executable's `__TEXT,__entitlements` section, as Xcode's does,
// not in its ad hoc signature.
const entitlements = (bundle) => readFileSync(join(bundle, plist(bundle, false).CFBundleExecutable), 'latin1').match(/<plist[^]*?<\/plist>/g)?.find((p) => p.includes('application-identifier')) ?? '';
const { open } = await import(resolve(root, 'scripts/agent.mjs'));
const texts = async (s) => {
  const { nodes } = await s.tree(), byId = new Map(nodes.map((n) => [n.id, n]));
  const text = (n) => n.props?.text ?? n.children.map((c) => text(byId.get(c))).join('');
  return Object.fromEntries(nodes.filter((n) => n.props?.testId).map((n) => [n.props.testId, text(n)]));
};
const until = async (s, want, ms = 10000) => {
  const end = Date.now() + ms;
  for (;;) {
    await s.clock('settle');
    const t = await texts(s);
    if (want(t) || Date.now() >= end) return t;
    await s.clock('+100 real');
  }
};

for (const host of hosts.length ? hosts : ['ios', 'macos']) {
  const where = host === 'ios' ? 'ios-simulator' : 'macos';
  grant(host === 'ios' ? 'device.shareplay' : '');
  const out = run('bun', ['exact.mjs', host === 'ios' ? 'ios' : 'mac', '--json', ...(host === 'ios' && sim ? ['--sim', sim] : [])], app);
  const bundle = JSON.parse(out.stdout.trim().split('\n').at(-1)).path;
  const info = plist(bundle, host === 'macos');
  check(where, 'no build takes cloudkit-share: neither CKSharingSupported nor ExactHostEvents', !('CKSharingSupported' in info) && !('ExactHostEvents' in info), JSON.stringify(Object.keys(info)));
  if (host === 'ios') check(where, 'device.shareplay signs Group Activities', /<key>com\.apple\.developer\.group-session<\/key>\s*<true\/>/.test(entitlements(bundle)), entitlements(bundle));
  const linked = run('otool', ['-L', join(bundle, host === 'ios' ? info.CFBundleExecutable : `Contents/MacOS/${info.CFBundleExecutable}`)], app).stdout;
  check(where, 'the host links neither CloudKit nor GroupActivities', !/CloudKit|GroupActivities/.test(linked), linked);

  const s = await open({ host, app: 'invitesfixture', ...(host === 'ios' && sim ? { sim } : {}) });
  try {
    const start = await until(s, (t) => t['at-start'] === '0', 3000);
    check(where, 'nothing was waiting when the instance started', start['at-start'] === '0' && start.taken === '', JSON.stringify(start));
    await s.tap('deliver');
    const one = await until(s, (t) => t.taken === 'invite 1');
    check(where, "a delivery reaches the session's instance through the mailbox", one.taken === 'invite 1' && one.count === '1', JSON.stringify(one));
    await s.tap('deliver');
    const two = await until(s, (t) => t.count === '2');
    check(where, 'each later one is taken once, in order', two.taken === 'invite 1; invite 2', JSON.stringify(two));
  } finally {
    await s.close();
  }
}
rmSync(dir, { recursive: true, force: true });
console.log(failures ? `${failures} failed` : 'all passed');
process.exit(failures ? 1 : 0);
