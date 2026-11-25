// `--attach` (LLP 1012.000): finding the running app's marker, on the Mac and in the iOS simulator, the attached wire,
// and what the driver refuses before it looks for an app. The app's side is driven on a Mac and a simulator (the
// RFC's piece 2 and 3 proofs); these need no app.
import { test, expect } from 'bun:test';
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { attachTarget, attachTargetIOS, attachedInput, devLoopOf, openAttached } from './agent-attach.mjs';
import { attachWire } from './agent-wire.mjs';
import { main } from './agent.mjs';

const START = 'Sun Nov 22 18:27:44 2026', started = Date.parse(START) / 1000 + 0.84;

/** A temporary directory holding markers and a private socket directory, and `ps`/`plutil` as a fake Mac answers. */
function machine({ processes = {}, plists = {} } = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'exact-attach-test-')), sockets = join(dir, 'exact-attach');
  mkdirSync(sockets, { mode: 0o700 });
  const marker = (pid, fields = {}, mode = 0o600) => {
    const socket = join(sockets, String(pid));
    writeFileSync(socket, '');
    writeFileSync(join(dir, `exact-attach-${pid}.json`), JSON.stringify({ v: 1, pid, started, bundle: 'com.example.notes', executable: `/Apps/Notes.app/Contents/MacOS/Notes`, socket, nonce: 'n', ...fields }), { mode });
    chmodSync(join(dir, `exact-attach-${pid}.json`), mode);
    return socket;
  };
  const run = (cmd, args) => {
    if (cmd === 'ps' && args[0] === '-axo') return { stdout: Object.entries(processes).map(([pid, p]) => `${pid} ${p.exe}`).join('\n') };
    if (cmd === 'ps') { const p = processes[args.at(-1)]; return { stdout: p ? (args[1] === 'comm=' ? p.exe : p.start) : '' }; }
    if (cmd === 'plutil') return { stdout: plists[args.at(-1)]?.[args[1]] ?? '' };
    return { stdout: '' };
  };
  return { dir, sockets, marker, run, done: () => rmSync(dir, { recursive: true, force: true }) };
}
const notes = { exe: '/Apps/Notes.app/Contents/MacOS/Notes', start: START };
const notesPlist = { '/Apps/Notes.app/Contents/Info.plist': { CFBundleIdentifier: 'com.example.notes', ExactBuildPurpose: 'local-development' } };
const find = (m, opts = {}) => attachTarget({ id: 'com.example.notes', name: 'Notes', app: 'notes', run: m.run, dir: m.dir, ...opts });

test('attach finds the marker of a live process started when it says, running the executable it names', async () => {
  const m = machine({ processes: { 41: notes }, plists: notesPlist });
  const server = createServer();
  try {
    const socket = m.marker(41);
    // A file where the socket should be is refused; the socket itself, private, is found.
    expect(() => find(m)).toThrow(`attach: ${socket} is not a socket in a directory only you can reach; not connecting`);
    rmSync(socket);
    await new Promise((r) => server.listen(socket, r));
    chmodSync(socket, 0o600);
    expect(find(m)).toMatchObject({ pid: 41, socket, nonce: 'n' });
    chmodSync(m.sockets, 0o755);
    expect(() => find(m)).toThrow('is not a socket in a directory only you can reach');
  } finally { server.close(); m.done(); }
});

test('attach skips a stale marker, a recycled PID, an open marker and another app', () => {
  const m = machine({
    processes: { 41: { ...notes, start: 'Sun Nov 22 18:29:00 2026' }, 42: { exe: '/bin/zsh', start: START }, 43: notes, 44: notes, 46: { exe: '/Apps/Other.app/Contents/MacOS/Other', start: START } },
    plists: notesPlist,
  });
  try {
    m.marker(41); // started later than the marker says: the PID was reused
    m.marker(42); // another executable
    m.marker(43, {}, 0o644); // readable by others
    m.marker(44, { bundle: 'com.example.other' });
    m.marker(45); // no such process
    m.marker(46, { executable: '/Apps/Other.app/Contents/MacOS/Other' }); // the executable's own bundle is another app's
    expect(() => find(m)).toThrow('attach: Notes pid 41 has not turned Agent Attach on: choose Develop ▸ Enable Agent Attach in its window');
  } finally { m.done(); }
});

test('attach asks for a PID among several, and says why a running build cannot be attached to', () => {
  const m = machine({ processes: { 41: notes, 42: notes }, plists: notesPlist });
  try {
    m.marker(41); m.marker(42);
    expect(() => find(m)).toThrow('attach: 2 running Notes processes have Agent Attach on (PIDs 41, 42): name one, --attach <pid>');
    expect(() => find(m, { pid: 42 })).toThrow('is not a socket in a directory only you can reach');
  } finally { m.done(); }
  const release = machine({ processes: { 50: notes }, plists: { '/Apps/Notes.app/Contents/Info.plist': { CFBundleIdentifier: 'com.example.notes', ExactBuildKind: 'release' } } });
  try {
    expect(() => find(release)).toThrow('attach: Notes pid 50 is a release build, which carries no agent attach: open a development build (`bun exact.mjs mac --run` in the app, or `bun host/apple/build.mjs notes --run` in exact2');
    expect(() => find(release, { pid: 51 })).toThrow('attach: pid 51 is not a running Notes (com.example.notes) on this Mac');
  } finally { release.done(); }
  const none = machine();
  try { expect(() => find(none)).toThrow('attach: no Notes (com.example.notes) is running on this Mac: open a development build'); }
  finally { none.done(); }
});

/** A fake app on a Unix socket: `answer(line, socket)` replies to each line the driver sends. */
async function fakeApp(answer) {
  const dir = mkdtempSync(join(tmpdir(), 'exact-attach-wire-')), path = join(dir, 's');
  const server = createServer((socket) => {
    let buffer = '';
    socket.setEncoding('utf8');
    socket.on('data', (d) => {
      buffer += d;
      let i;
      while ((i = buffer.indexOf('\n')) >= 0) { const line = JSON.parse(buffer.slice(0, i)); buffer = buffer.slice(i + 1); answer(line, socket); }
    });
  });
  await new Promise((r) => server.listen(path, r));
  const { connect } = await import('node:net');
  return { socket: () => connect({ path }), close: () => { server.close(); rmSync(dir, { recursive: true, force: true }); } };
}
const send = (socket, object) => socket.write(JSON.stringify(object) + '\n');

test('the attached wire waits for the person, then matches each reply to its request', async () => {
  const app = await fakeApp((line, socket) => {
    if (line.attach) { send(socket, { attach: 1, waiting: 'approval', window: 'Notes', pid: 41 }); setTimeout(() => send(socket, { attached: true, pid: 41, window: 'Notes' }), 20); }
    else send(socket, { request: line.request, op: line.op, id: line.id ?? null });
  });
  try {
    let waited = null;
    const wire = await attachWire(app.socket(), { nonce: 'n', onWaiting: (w) => { waited = w.window; } });
    expect(waited).toBe('Notes');
    expect(wire.attached.pid).toBe(41);
    // `id` names a view on this wire; the request's number is `request`, and concurrent asks go one at a time.
    const [a, b] = await Promise.all([wire.ask({ op: 'layout', id: 36 }), wire.ask({ op: 'tree' })]);
    expect(a).toEqual({ op: 'layout', id: 36 });
    expect(b).toEqual({ op: 'tree', id: null });
    wire.close();
  } finally { app.close(); }
});

test('the attached wire says a refusal, and a late reply never answers the next request', async () => {
  const refusing = await fakeApp((line, socket) => { send(socket, { refused: 'busy', error: 'busy: another agent is attached to this app' }); socket.end(); });
  try { await expect(attachWire(refusing.socket(), { nonce: 'n' })).rejects.toThrow('attach: busy: another agent is attached to this app'); }
  finally { refusing.close(); }
  const slow = await fakeApp((line, socket) => {
    if (line.attach) return send(socket, { attached: true });
    setTimeout(() => send(socket, { request: line.request }), line.op === 'state' ? 200 : 0);
  });
  try {
    const wire = await attachWire(slow.socket(), { nonce: 'n', requestMs: 50 });
    await expect(wire.ask({ op: 'state' })).rejects.toThrow('the app did not answer state (request 1) within 0.05 s, so the connection was closed');
    await expect(wire.ask({ op: 'tree' })).rejects.toThrow('within 0.05 s');
  } finally { slow.close(); }
  const wide = await fakeApp((line, socket) => send(socket, line.attach ? { attached: true } : { request: line.request, text: 'é'.repeat(40) }));
  try {
    const wire = await attachWire(wide.socket(), { nonce: 'n', lineLimit: 70 });
    await expect(wire.ask({ op: 'tree' })).rejects.toThrow('the app sent a reply over 32 MiB'); // 80 bytes of text in 40 characters
  } finally { wide.close(); }
  const crossed = await fakeApp((line, socket) => send(socket, line.attach ? { attached: true } : { request: 7 }));
  try {
    const wire = await attachWire(crossed.socket(), { nonce: 'n' });
    await expect(wire.ask({ op: 'tree' })).rejects.toThrow('the app answered request 7 where 1 was asked');
  } finally { crossed.close(); }
});

test('--attach refuses what only a launch takes, and a host with no attach, before it looks for an app', async () => {
  const opened = [];
  const fail = { open: async (o) => { opened.push(o); throw new Error('opened'); }, driveServer: async () => { throw new Error('a server was started'); } };
  await expect(main(['macos', '--attach', '--storage', 'x', '--seed', '2', 'tree'], fail)).rejects.toThrow('--attach reads an app that is already running: --storage, --seed are for a launch the driver owns');
  await expect(main(['macos', '--attach', '--live', 'tree'], fail)).rejects.toThrow('--live is for a launch');
  await expect(main(['macos', '--attach', '--headless', 'tree'], fail)).rejects.toThrow('--headless is for a launch');
  await expect(main(['macos', '--attach', '--call', 'x {}', 'tree'], fail)).rejects.toThrow('--notification, --wake, --call and --login is for a launch');
  for (const host of ['android', 'windows', 'linux']) await expect(main([host, '--attach', 'tree'], fail)).rejects.toThrow(`--attach: unsupported on ${host}: attach on ${host} is not built yet`);
  await expect(main(['tvos', '--attach', 'tree'], fail)).rejects.toThrow('--attach: unsupported on tvos: an Apple TV build carries no attach');
  await expect(main(['watchos', '--attach', 'tree'], fail)).rejects.toThrow('a watch app carries no attach');
  await expect(main(['host-ios', '--attach', 'tree'], fail)).rejects.toThrow('the embedding fixture carries no attach');
  for (const device of [['--device'], ['--device', '--phone', 'X']]) await expect(main(['ios', '--attach', ...device, 'tree'], fail)).rejects.toThrow('--attach: unsupported on a physical iPhone');
  await expect(main(['macos', '--attach', '--sim', 'X', 'tree'], fail)).rejects.toThrow('--sim is for a launch');
  await expect(main(['trace', '--attach'], fail)).rejects.toThrow('--attach drives a running app');
  expect(opened).toEqual([]);
  await expect(main(['macos', '--app', 'caltrain', '--attach', '41', '--session', 'b', 'tree'], fail)).rejects.toThrow('opened');
  await expect(main(['ios', '--app', 'caltrain', '--attach', '--sim', 'X', 'tree'], fail)).rejects.toThrow('opened');
  await expect(main(['web', '--app', 'caltrain', '--attach', '--url', 'http://127.0.0.1:1/', 'tree'], fail)).rejects.toThrow('--url is for a launch');
  await expect(main(['web', '--app', 'caltrain', '--attach', 'tree'], fail)).rejects.toThrow('opened');
  expect(opened).toEqual([{ host: 'macos', app: 'caltrain', session: 'b', attach: 41 }, { host: 'ios', app: 'caltrain', session: undefined, attach: true }, { host: 'web', app: 'caltrain', session: undefined, attach: true }]);
});

/** A simulator `udid`'s app container holding markers, and the private socket directory, as `simctl` and `ps` answer. */
const UDID = '11111111-2222-3333-4444-555555555555', OTHER = '66666666-7777-8888-9999-000000000000';
function simulatorMachine({ processes = {}, plists = {}, installed = true } = {}) {
  const m = machine({ processes, plists });
  mkdirSync(join(m.dir, 'tmp'));
  const run = (cmd, args) => {
    if (cmd === 'xcrun') return installed ? { status: 0, stdout: `${m.dir}\n` } : { status: 2, stdout: '', stderr: 'No such file' };
    return m.run(cmd, args);
  };
  const marker = (pid, fields = {}) => {
    const socket = join(m.sockets, String(pid));
    writeFileSync(join(m.dir, 'tmp', `exact-attach-${pid}.json`), JSON.stringify({ v: 1, pid, started, bundle: 'com.example.notes', executable: appExe(UDID), socket, nonce: 'n', ...fields }), { mode: 0o600 });
    return socket;
  };
  return { ...m, run, marker };
}
const appDir = (udid) => `/Users/me/Library/Developer/CoreSimulator/Devices/${udid}/data/Containers/Bundle/Application/ABCDEFAB-CDEF-ABCD-EFAB-CDEFABCDEFAB/Notes.app`;
const appExe = (udid) => `${appDir(udid)}/Notes`;
const simPlists = (purpose = 'local-development') => Object.fromEntries([UDID, OTHER].map((u) => [`${appDir(u)}/Info.plist`, { CFBundleIdentifier: 'com.example.notes', ExactBuildPurpose: purpose }]));
const iphone = (state = 'Booted', runtime = 'com.apple.CoreSimulator.SimRuntime.iOS-26-5') => () => ({ udid: UDID, name: 'iPhone 17', state, runtime });
const findIOS = (m, opts = {}) => attachTargetIOS({ id: 'com.example.notes', name: 'Notes', app: 'notes', run: m.run, choose: iphone(), ...opts });

test('attach finds the simulator app\'s marker in its container on the selected device', async () => {
  const m = simulatorMachine({ processes: { 61: { exe: appExe(UDID), start: START } }, plists: simPlists() });
  const server = createServer();
  try {
    const socket = m.marker(61);
    await new Promise((r) => server.listen(socket, r));
    chmodSync(socket, 0o600);
    expect(findIOS(m)).toMatchObject({ pid: 61, socket, device: { udid: UDID } });
    // The same marker naming an executable on another device is not this device's app.
    m.marker(61, { executable: appExe(OTHER) });
    expect(() => findIOS(m)).toThrow('Notes pid 61 in simulator iPhone 17');
  } finally { server.close(); m.done(); }
});

test('attach in the simulator refuses another device, an Apple TV, a simulator not running and a build without attach', () => {
  const elsewhere = simulatorMachine({ processes: { 62: { exe: appExe(OTHER), start: START } }, plists: simPlists() });
  try { expect(() => findIOS(elsewhere)).toThrow(`attach: Notes is not running in the selected simulator iPhone 17 (${UDID}); it runs in ${OTHER}: name that one with --sim <udid>`); }
  finally { elsewhere.done(); }
  const m = simulatorMachine({ processes: { 63: { exe: appExe(UDID), start: START } }, plists: simPlists() });
  try {
    expect(() => findIOS(m)).toThrow('Notes pid 63 in simulator iPhone 17 (11111111-2222-3333-4444-555555555555) has not turned Agent Attach on: open its developer menu (four fingers, or ⌘D) and choose Enable Agent Attach');
    expect(() => findIOS(m, { choose: iphone('Shutdown') })).toThrow('is not running: build and open it there');
    expect(() => findIOS(m, { choose: iphone('Booted', 'com.apple.CoreSimulator.SimRuntime.tvOS-26-5') })).toThrow('--attach: unsupported on tvOS (simulator iPhone 17): an Apple TV build carries no attach');
  } finally { m.done(); }
  const release = simulatorMachine({ processes: { 64: { exe: appExe(UDID), start: START } }, plists: simPlists(null) });
  try { expect(() => findIOS(release)).toThrow('Notes pid 64 in simulator iPhone 17 (11111111-2222-3333-4444-555555555555) is a unstamped build, which carries no agent attach'); }
  finally { release.done(); }
  const missing = simulatorMachine({ installed: false });
  try { expect(() => findIOS(missing)).toThrow('Notes (com.example.notes) is not installed in simulator iPhone 17'); }
  finally { missing.done(); }
});

test('the attached carrier refuses a physical device before it looks for an app', async () => {
  const looked = [];
  const find = (o) => { looked.push(o); throw new Error('looked'); };
  await expect(openAttached({ host: 'ios', app: 'caltrain', device: true, find })).rejects.toThrow('--attach: unsupported on a physical iPhone');
  expect(looked).toEqual([]);
  // A simulator's lookup is given the app's directory, whose remembered simulator a drive also chooses.
  await expect(openAttached({ host: 'ios', app: 'caltrain', find })).rejects.toThrow('looked');
  expect(looked[0]).toMatchObject({ id: 'com.exact.caltrain', app: 'caltrain', appDir: expect.stringMatching(/apps\/caltrain$/) });
  expect(looked[0].dir).toBeUndefined(); // the Mac lookup's `dir` is its temporary directory
});

/** A carrier whose app answers each request with `answer(req)`, recording them. */
function inputCarrier(host, answer, touches) {
  const sent = [];
  return { sent, carrier: { host, attached: {}, ask: async (req) => { sent.push(req); return answer(req); }, touches: async () => touches, abandon() {} } };
}

test('an attached tap and type are plain: a point, a button, a key or a gesture is refused before anything is sent', async () => {
  const { sent, carrier } = inputCarrier('macos', (req) => ({ tapped: 12, delivery: 'platform', ...(req.op === 'type' ? { typed: 12, value: req.text } : {}) }));
  const input = attachedInput(carrier);
  for (const opts of [{ at: [1, 2] }, { dblclick: true }, { modifiers: 'Shift' }, { drag: { dx: 1 } }, { fingers: 4 }, { down: true }])
    await expect(input.tap('goal-up', opts)).rejects.toThrow('tap: unsupported: an attached session taps plainly');
  for (const text of [{ key: 'Enter' }, { clipboard: 'paste', text: 'x' }]) await expect(input.type('notes', text)).rejects.toThrow('type: unsupported');
  expect(sent).toEqual([]);
  expect(await input.tap('#12')).toMatchObject({ tapped: 12 });
  expect(await input.type('notes', 'hello')).toMatchObject({ typed: 12, value: 'hello' });
  expect(sent).toEqual([{ op: 'tap', target: 12 }, { op: 'type', target: 'notes', text: 'hello' }]);
  const { carrier: refusing } = inputCarrier('macos', () => ({ error: 'input-not-granted: this connection may read', refused: 'input-not-granted' }));
  await expect(attachedInput(refusing).tap('goal-up')).rejects.toThrow('tap: input-not-granted');
});

test('in the simulator the app aims and the touch runner plays the touch, checked in the window\'s log', async () => {
  const aim = { orientation: 'portrait', session: 'main', generation: 1, screen: { x: 0, y: 0, w: 400, h: 800 }, point: [30, 20], fixed: [30, 20], fixedAxes: [[1, 0], [0, 1]], display: [400, 800], viewport: [400, 800], at: [30, 20], seq: 4, hit: 7 };
  const touch = (phase, seq) => ({ seq, touch: 1, phase, t: seq, at: [30, 20], node: 7, session: 'main', generation: 1 });
  const played = [];
  const runner = { async ask(req) { played.push(req); return req.op === 'foreground' ? { state: 'runningForeground' } : { injected: {}, done: true }; } };
  const { sent, carrier } = inputCarrier('ios', (req) => (req.aim ? { aim, id: 7 } : { log: [touch('began', 5), touch('ended', 6)], truncated: false }), runner);
  expect(await attachedInput(carrier).tap('goal-up')).toMatchObject({ tapped: 7, delivery: 'platform' });
  expect(sent[0]).toEqual({ op: 'tap', target: 'goal-up', aim: { press: true } });
  // Every aim names the target as given, never the view id the first resolved it to.
  expect(sent.filter((r) => r.aim).every((r) => r.target === 'goal-up' && r.id === undefined)).toBe(true);
  expect(sent.some((r) => r.log !== undefined)).toBe(true);
  expect(played.find((r) => r.op === 'tap')).toMatchObject({ op: 'tap', screen: [30, 20], display: [400, 800] });
  // A refused aim plays nothing.
  played.length = 0;
  const { carrier: refused } = inputCarrier('ios', () => ({ error: 'person-input: a finger is down in the app', refused: 'person-input' }), runner);
  await expect(attachedInput(refused).tap('goal-up')).rejects.toThrow('tap: person-input');
  expect(played).toEqual([]);
});

test('a tap whose reply is lost after it was sent is reported unknown, and nothing is sent again', async () => {
  const seen = [];
  const app = await fakeApp((line, socket) => {
    seen.push(line);
    if (line.attach) send(socket, { attached: true, pid: 41 });
    else if (line.op === 'tap') socket.destroy();
  });
  try {
    const carrier = await openAttached({ host: 'macos', app: 'caltrain', find: () => ({ socket: null, nonce: 'n', pid: 41 }), dial: () => app.socket(), say: () => {} });
    await expect(carrier.ask({ op: 'tap', target: 'goal-up' })).rejects.toThrow(/tap: the connection ended after the request was sent .*whether it took effect is unknown, and it was not sent again/);
    await expect(carrier.ask({ op: 'tree' })).rejects.toThrow('the app closed the attach connection');
    await expect(carrier.ask({ op: 'pick', target: 'x' })).rejects.toThrow('pick: unsupported');
    expect(seen.filter((l) => l.op === 'tap')).toHaveLength(1);
  } finally { app.close(); }
});

test('in the simulator a target that resolves elsewhere plays nothing, and a text entry is checked before its tap and its keys', async () => {
  const aim = { orientation: 'portrait', session: 'main', generation: 1, screen: { x: 0, y: 0, w: 400, h: 800 }, point: [30, 20], fixed: [30, 20], fixedAxes: [[1, 0], [0, 1]], display: [400, 800], viewport: [400, 800], at: [30, 20], seq: 4, hit: 7 };
  const played = [];
  const runner = { async ask(req) { played.push(req); return req.op === 'foreground' ? { state: 'runningForeground' } : { injected: {}, done: true }; } };
  let aims = 0;
  const moving = inputCarrier('ios', (req) => (req.aim ? { aim, id: ++aims === 1 ? 7 : 9 } : { log: [], truncated: false }), runner);
  await expect(attachedInput(moving.carrier).tap('goal-up')).rejects.toThrow('goal-up is now view #9, not #7; nothing was played');
  expect(played.filter((r) => r.op === 'tap')).toEqual([]);
  const notText = inputCarrier('ios', (req) => (req.editable ? { error: 'target: view #7 is not an editable text input', refused: 'target' } : {}), runner);
  await expect(attachedInput(notText.carrier).type('goal-up', 'x')).rejects.toThrow('not an editable text input');
  expect(notText.sent).toEqual([{ op: 'type', target: 'goal-up', editable: true }]);
  await expect(attachedInput(notText.carrier).type('notes', 'x'.repeat(501))).rejects.toThrow('at most 500 characters');
});

test('a web tab is found by its app\'s running JS-target loop and chosen by its code, never a PID or a session', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-attach-loop-'));
  try {
    expect(() => devLoopOf(dir, 'notes')).toThrow('no web dev loop of notes runs on this machine');
    mkdirSync(join(dir, '.exact'));
    writeFileSync(join(dir, '.exact/web-wasm-loop.json'), '{}');
    expect(() => devLoopOf(dir, 'notes')).toThrow('runs the wasm target');
    writeFileSync(join(dir, '.exact/web-js-loop.json'), JSON.stringify({ pid: process.pid, url: null }));
    expect(() => devLoopOf(dir, 'notes')).toThrow('is still starting');
    writeFileSync(join(dir, '.exact/web-js-loop.json'), JSON.stringify({ pid: 2 ** 22 + 7, url: 'http://127.0.0.1:8765' }));
    expect(() => devLoopOf(dir, 'notes')).toThrow('is not running');
    writeFileSync(join(dir, '.exact/web-js-loop.json'), JSON.stringify({ pid: process.pid, url: 'http://10.0.0.2:8765' }));
    expect(() => devLoopOf(dir, 'notes')).toThrow('not this machine\'s loopback');
    writeFileSync(join(dir, '.exact/web-js-loop.json'), JSON.stringify({ pid: process.pid, url: 'http://127.0.0.1:8765' }));
    expect(devLoopOf(dir, 'notes')).toEqual({ pid: process.pid, port: 8765 });
  } finally { rmSync(dir, { recursive: true, force: true }); }
  await expect(openAttached({ host: 'web', app: 'caltrain', pid: 41 })).rejects.toThrow('a web tab is chosen by the pairing code it shows, not a PID');
  await expect(openAttached({ host: 'web', app: 'caltrain', session: 'b' })).rejects.toThrow('an attached tab has one session');
});
