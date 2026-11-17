import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const target = fileURLToPath(new URL('../../target/', import.meta.url));

/** Runs a copy of round.mjs, outside this checkout so `report` writes no results here, with HOME at `home`. */
function withRound(check) {
  mkdirSync(target, { recursive: true });
  const root = mkdtempSync(join(target, 'bakeoff-round-'));
  try {
    const here = join(root, 'bench/bakeoff');
    mkdirSync(here, { recursive: true });
    for (const f of ['round.mjs', 'report.mjs', 'pairs.json']) copyFileSync(new URL(`./${f}`, import.meta.url), join(here, f));
    const home = join(root, 'home');
    const write = (path, text = '') => { mkdirSync(dirname(join(home, path)), { recursive: true }); writeFileSync(join(home, path), text); };
    const run = (name, phase, env = {}) => {
      const r = spawnSync(process.execPath, [join(here, 'round.mjs'), name, phase], { env: { ...process.env, HOME: home, ...env }, encoding: 'utf8' });
      assert.equal(r.status, 0, r.stderr);
    };
    check({ here, home, write, run });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

const files = (dir) => readdirSync(dir, { recursive: true, withFileTypes: true })
  .filter((f) => f.isFile()).map((f) => relative(dir, join(f.parentPath, f.name))).sort();

test('report copies a grade\'s records and note, not what the grader wrote to drive the apps', () => withRound(({ here, write, run }) => {
  const spec = 'bakeoff/rounds/zz/grades/chat';
  for (const f of ['grade.json', 'key.json', 'meta.json']) write(`${spec}/${f}`, '{}\n');
  write(`${spec}/GRADING-ABORTED.md`, 'The simulator would not boot.\n');
  for (const f of ['prompt.md', 'log.jsonl', 'shots/A-1.png', 'd.sh', '.tools/d.sh', 'bin/A']) write(`${spec}/${f}`, '#!/bin/sh\n');
  write('bakeoff/rounds/zz/grades/chat-tools/d', '#!/bin/sh\n');
  run('zz', 'report');
  assert.deepEqual(files(join(here, 'results/zz/grades')), ['chat/GRADING-ABORTED.md', 'chat/grade.json', 'chat/key.json', 'chat/meta.json']);
}));

test('tidy keeps the pin another running round uses', () => withRound(({ home, write, run }) => {
  for (const rev of ['aaa', 'bbb', 'ccc', 'ddd']) write(`bakeoff/pins/${rev}/README.md`);
  write('bakeoff/rounds/b/pin.txt', 'bbb\n');
  // A round building on aaa (this test's process stands in for it), and one on ccc that died without letting go.
  write(`bakeoff/locks/pin-aaa-${process.pid}`);
  write(`bakeoff/locks/pin-ccc-${spawnSync('true').pid}`);
  // A `git` that fails, so tidy deletes each pin it would remove as a worktree.
  write('bin/git', '#!/bin/sh\nexit 1\n');
  chmodSync(join(home, 'bin/git'), 0o755);
  run('b', 'tidy', { PATH: `${join(home, 'bin')}:${process.env.PATH}` });
  assert.deepEqual(readdirSync(join(home, 'bakeoff/pins')).sort(), ['aaa', 'bbb']);
  assert.deepEqual(readdirSync(join(home, 'bakeoff/locks')), [`pin-aaa-${process.pid}`]);
}));
