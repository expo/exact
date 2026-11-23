// `--attach` (LLP 1012.000): finding the running app's marker, the attached wire, and what the driver refuses before
// it looks for an app. The app's side is driven on a Mac (the RFC's piece 2 proof); these need no app.
import { test, expect } from 'bun:test';
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { attachTarget } from './agent-inspect.mjs';
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
  await expect(main(['android', '--attach', 'tree'], fail)).rejects.toThrow('--attach: unsupported on android');
  await expect(main(['ios', '--attach', 'tree'], fail)).rejects.toThrow('the iOS simulator is not built yet');
  await expect(main(['trace', '--attach'], fail)).rejects.toThrow('--attach drives a running app');
  expect(opened).toEqual([]);
  await expect(main(['macos', '--app', 'caltrain', '--attach', '41', '--session', 'b', 'tree'], fail)).rejects.toThrow('opened');
  expect(opened).toEqual([{ host: 'macos', app: 'caltrain', session: 'b', attach: 41 }]);
});
