// LLP 1069.010.000 D4's hard rows driven through the kept editor
// (app.contract): a Markdown editor saving the document the person chose
// with its own atomic replace, and another program replacing that file.
//
// Every host: the editor's save lands and the document reads at the same
// `doc:` path. Where the file is on this machine's disk (Linux, macOS),
// another program then replaces it at its path mid-session: the watch shows
// its text (not on the Linux host built for macOS, which has no inotify),
// and the editor's save of a draft begun from the earlier file is refused
// `ECHANGED`, leaving the other program's file. On the web the
// driver hands the page the file's bytes, so no other program can reach it:
// host/web/documents-kept.test.mjs drives the same rows over real handles.
// macOS keeps what a drive chose (its scratch store): after a relaunch the
// kept path reads the editor's save, and once another program has replaced
// the file the next launch has forgotten it (ENOENT). A Linux drive and a
// web drive keep nothing (D6), so they show the rows within the session.
//
// usage: bun scripts/fixtures/kept-editor/kept.drives.mjs <web|linux|macos>, the app built first with
//   EXACT_APP_DIR=scripts/fixtures/kept-editor set: host/web/build.mjs kept-editor-web,
//   scripts/build-linux.mjs kept-editor, or host/apple/build.mjs kept-editor-apple
import { mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';

const root = resolve(import.meta.dir, '../../..');
const host = process.argv[2] ?? 'web';
const app = resolve(import.meta.dir);
let failures = 0;
function check(what, ok, detail = '') {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${host}: ${what}${ok ? '' : `\n     ${String(detail).slice(0, 800)}`}`);
  if (!ok) failures++;
}

/** One launch: each op's JSON reply, in order. `during(i)` runs as reply `i` arrives. */
function launch(storage, ops, { reset = true, during = () => {} } = {}) {
  const args = ['scripts/agent.mjs', host, '--json', '--storage', storage, ...(reset ? ['--reset-storage'] : []), ...ops];
  const child = spawn('bun', args, { cwd: root, env: { ...process.env, EXACT_APP_DIR: app } });
  const replies = [];
  let out = '', err = '';
  child.stderr.on('data', d => { err += d; });
  child.stdout.on('data', d => {
    out += d;
    for (let at; (at = out.indexOf('\n')) >= 0;) {
      const line = out.slice(0, at);
      out = out.slice(at + 1);
      if (!/^[[{"]|^(null|true|false|-?\d)/.test(line)) continue;
      replies.push(JSON.parse(line));
      during(replies.length - 1);
    }
  });
  return new Promise(done => child.on('close', () => {
    if (replies.length < ops.length) console.log(err.slice(-3000));
    done(replies);
  }));
}
/** What a `tree` reply shows for a testId: a field's value, else its text. */
const shown = (tree, id) => (tree?.nodes ?? []).filter(n => n.props?.testId === id).map(n => n.props.value ?? n.props.text ?? n.text ?? '');
const one = (tree, id) => shown(tree, id)[0];

const dir = mkdtempSync(join(tmpdir(), 'kept-editor-'));
const file = join(dir, 'notes.md'), run = `kept-${Date.now().toString(36)}`;
writeFileSync(file, '# Notes\n\nfirst\n');
/** Another program's atomic replace: a new file renamed over the path, as a sync client or another editor saves. */
function replace(text) {
  writeFileSync(`${file}.theirs`, text);
  renameSync(`${file}.theirs`, file);
}
const opened = ['tap choose', `type @open-file ${file}`, 'clock settle', 'tree'];
const saved = (text) => [`type editor ${text}`, 'tap save', 'clock settle', 'tree'];

// The editor's own atomic save, then (on disk) another program's replacement under a draft.
const first = await launch(`${run}-1`, [...opened, ...saved('# Notes\n\nmine')]);
const path = one(first[3], 'path') ?? '';
check('the chosen file opens at a doc: path', /^doc:\/\d+\/notes\.md$/.test(path) && one(first[3], 'on-disk') === '# Notes\n\nfirst\n', JSON.stringify(first[3]));
check("the editor's atomic save lands and reads back at that path", one(first[7], 'saved') === 'saved' && one(first[7], 'on-disk') === '# Notes\n\nmine' && one(first[7], 'path') === path, JSON.stringify(first[7]));
if (host !== 'web') check('the file on disk is the save', readFileSync(file, 'utf8') === '# Notes\n\nmine');

/** In a session holding `path`: a draft begun, the file replaced by another program, the draft's save refused. */
async function replaced(storage, before, reset) {
  const ops = [...before, 'type editor # Notes\n\nmine, again', 'clock +3000 real', 'clock settle', 'tree', 'tap save', 'clock settle', 'tree'];
  const draft = before.length;
  const r = await launch(storage, ops, { reset, during: i => { if (i === draft) replace('# Theirs\n'); } });
  const n = r.length;
  const seen = (tree) => JSON.stringify(['on-disk', 'editor', 'saved', 'error'].map(id => one(tree, id)));
  // The Linux host built for macOS has no inotify: its watch never announces (host/painted/src/doc_watch.rs).
  if (host === 'linux' && process.platform !== 'linux') console.log('skip linux: the watch, which the Linux host built for macOS does not announce');
  else check("another program's replacement shows through the watch, under the draft", one(r[draft + 3], 'on-disk') === '# Theirs\n' && one(r[draft + 3], 'editor') === '# Notes\n\nmine, again', seen(r[draft + 3]));
  check('a save of the draft begun from the earlier file is refused ECHANGED, the draft kept', one(r[n - 1], 'saved') === 'ECHANGED' && one(r[n - 1], 'editor') === '# Notes\n\nmine, again', seen(r[n - 1]));
  check("the path reads the other program's file, which stays", one(r[n - 1], 'on-disk') === '# Theirs\n' && readFileSync(file, 'utf8') === '# Theirs\n', seen(r[n - 1]));
  return r;
}

if (host === 'linux') await replaced(`${run}-1`, [...opened], true);
if (host === 'macos') {
  // Kept: a relaunch lists the path and reads the editor's save through it, no picker.
  const reopen = [`type open-file ${path}`, 'type open-file key Enter', 'clock settle', 'tree'];
  const second = await replaced(`${run}-1`, reopen, false);
  check('after a relaunch the kept path is listed and reads the save', shown(second[3], 'kept').includes(path) && one(second[3], 'on-disk') === '# Notes\n\nmine', JSON.stringify(second[3]));
  // Replaced by another program, it is not the document chosen: the next launch has forgotten it.
  const third = await launch(`${run}-1`, ['clock settle', ...reopen], { reset: false });
  check('replaced by another program, it is forgotten at the next launch', !shown(third[4], 'kept').includes(path) && one(third[4], 'error') === 'ENOENT', JSON.stringify(third[4]));
}
rmSync(dir, { recursive: true, force: true });
console.log(`${failures} failed`);
process.exit(failures ? 1 : 0);
