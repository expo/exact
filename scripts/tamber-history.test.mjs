// History and revert through the design server (LLP 1122 D9, T1).
import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { developmentGate } from './install-page.mjs';
import { designLog } from './tamber-history.mjs';
import { tamberServer } from './tamber-server.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));

const PARAM_DESIGN = 'tamber t\n\naxes\n  dark-style = black | dim\n\ntheme\n  ink = #000000\n  edge = #101010\n\nparams\n  PostButton.radius = 8px\n    when dark-style=dim: 4px\n';

async function paramHistory(before, after, run) {
  const repo = mkdtempSync(join(tmpdir(), 'exact-tamber-param-history-'));
  const env = { ...process.env, GIT_AUTHOR_NAME: 'Ada', GIT_AUTHOR_EMAIL: 'ada@example.test', GIT_COMMITTER_NAME: 'Ada', GIT_COMMITTER_EMAIL: 'ada@example.test' };
  const g = (...args) => {
    const r = spawnSync('git', args, { cwd: repo, encoding: 'utf8', env });
    expect(r.status, r.stderr).toBe(0);
    return r.stdout.trim();
  };
  let wire, listener;
  try {
    writeFileSync(join(repo, 'app.contract'), 'use PostButton from "./post.contract"\n\ncomponent App\n  view\n    PostButton()\n');
    writeFileSync(join(repo, 'post.contract'), 'component PostButton\n  params\n    radius: length = "0px"\n  view\n    column border-radius="param.radius"\n      text "Post"\n');
    g('init', '-q');
    for (const text of [before, after]) {
      writeFileSync(join(repo, 'app.tamber'), text);
      g('add', '.'); g('commit', '-q', '-m', 'design');
    }
    const commit = g('rev-parse', 'HEAD');
    listener = createServer();
    await new Promise(ok => listener.listen(0, '127.0.0.1', ok));
    const port = listener.address().port;
    const at = `http://127.0.0.1:${port}`;
    const gate = developmentGate([{ origin: at }], port);
    wire = tamberServer({ appDir: repo, log: () => {}, settle: 250 });
    listener.on('request', async (req, res) => {
      if (!(await wire.handle(req, res, new URL(req.url, at), gate.check(req)))) { res.writeHead(404); res.end(); }
    });
    await wire.ready;
    const { token } = await (await fetch(`${at}/__tamber/token`)).json();
    const call = async request => {
      const res = await fetch(`${at}/__tamber`, { method: 'POST', headers: { 'x-exact-tamber-token': token }, body: JSON.stringify({ client: 'ada', ...request }) });
      return { status: res.status, ...(await res.json()) };
    };
    const read = () => readFileSync(join(repo, 'app.tamber'), 'utf8');
    const change = async op => {
      const files = { 'app.tamber': createHash('sha256').update(read()).digest('hex') };
      const r = await call({ method: 'transaction', client: 'agent', files, ops: [op] });
      expect(r.status, JSON.stringify(r)).toBe(200);
    };
    await run({ call, commit, read, change });
  } finally {
    await wire?.close();
    listener?.close();
    rmSync(repo, { recursive: true, force: true });
  }
}

test.each([
  ['PostButton.radius', 'post.contract#PostButton.radius'],
  ['post.contract#PostButton.radius', 'PostButton.radius'],
])('a param revert refuses a spelling change from %s to %s', async (oldName, newName) => {
  const before = PARAM_DESIGN.replace('PostButton.radius', oldName);
  const after = PARAM_DESIGN.replace('PostButton.radius', newName).replace('8px', '20px');
  await paramHistory(before, after, async ({ call, commit, read }) => {
    const r = await call({ method: 'revert', commit });
    expect(r.status, JSON.stringify(r)).toBe(409);
    expect(r.error.refused.map(c => c.edit.name)).toEqual(expect.arrayContaining([oldName, newName]));
    expect(r.error.refused.every(c => c.why.includes('spelling'))).toBe(true);
    expect(read()).toBe(after);
  });
}, 120_000);

test.each(['default', 'set-when', 'unset-when'])('a partial param revert keeps only its %s after the file moves', async branch => {
  const before = branch === 'unset-when' ? PARAM_DESIGN.replace('    when dark-style=dim: 4px\n', '') : PARAM_DESIGN;
  const after = PARAM_DESIGN.replace('8px', '20px').replace('4px', '12px').replace('#000000', '#111111');
  await paramHistory(before, after, async ({ call, commit, read, change }) => {
    const held = await call({ method: 'revert', commit });
    expect(held.status, JSON.stringify(held)).toBe(200);
    const item = held.ok.ops.find(o => o.op.name === 'PostButton.radius' && !!o.op.when.length === (branch !== 'default'));
    expect(item.op.op).toBe(branch === 'unset-when' ? 'unset-param' : 'set-param');
    const part = await call({ method: 'accept', proposal: held.ok.proposal, items: [item.index] });
    expect(part.status, JSON.stringify(part)).toBe(200);
    expect(part.ok.ops.map(o => o.op)).toEqual([item.op]);
    await change({ op: 'set-token', name: 'edge', value: '#303030' });
    const again = await call({ method: 'accept', proposal: part.ok.proposal });
    expect(again.ok?.recomputed, JSON.stringify(again)).toBe(true);
    expect(again.ok.ops.map(o => o.op)).toEqual([item.op]);
    expect((await call({ method: 'accept', proposal: again.ok.proposal })).status).toBe(200);
    const expected = branch === 'default' ? after.replace('20px', '8px')
      : branch === 'set-when' ? after.replace('12px', '4px') : after.replace('    when dark-style=dim: 12px\n', '');
    expect(read()).toBe(expected.replace('#101010', '#303030'));
  });
}, 120_000);

test.each([false, true])('a partial param revert reports selected conflicts with another kept edit: %s', async keepInk => {
  const after = PARAM_DESIGN.replace('8px', '20px').replace('4px', '12px').replace('#000000', '#111111');
  await paramHistory(PARAM_DESIGN, after, async ({ call, commit, read, change }) => {
    for (const when of [[], [{ axis: 'dark-style', value: 'dim' }]]) {
      const held = await call({ method: 'revert', commit });
      expect(held.status, JSON.stringify(held)).toBe(200);
      const items = held.ok.ops.filter(o => (keepInk && o.op.name === 'ink') || (o.op.name === 'PostButton.radius' && o.op.when.length === when.length)).map(o => o.index);
      const part = await call({ method: 'accept', proposal: held.ok.proposal, items });
      expect(part.status, JSON.stringify(part)).toBe(200);
      await change({ op: 'set-param', name: 'PostButton.radius', when, value: '30px' });
      const moved = read();
      const refused = await call({ method: 'accept', proposal: part.ok.proposal });
      expect(refused.status, JSON.stringify(refused)).toBe(409);
      expect(refused.error.refused).toMatchObject([{ edit: { what: 'param', name: 'PostButton.radius', when }, now: '30px', why: 'changed since' }]);
      expect(read()).toBe(moved);
      await change({ op: 'set-param', name: 'PostButton.radius', when, value: when.length ? '12px' : '20px' });
    }
  });
}, 120_000);

test('a removed param revert reports a selected conditional conflict after the file moves', async () => {
  const after = PARAM_DESIGN.slice(0, PARAM_DESIGN.indexOf('\nparams\n')).replace('#000000', '#111111');
  await paramHistory(PARAM_DESIGN, after, async ({ call, commit, read, change }) => {
    const held = await call({ method: 'revert', commit });
    expect(held.status, JSON.stringify(held)).toBe(200);
    const when = [{ axis: 'dark-style', value: 'dim' }];
    const items = held.ok.ops.filter(o => o.op.name === 'ink' || (o.op.name === 'PostButton.radius' && o.op.when.length)).map(o => o.index);
    expect(items).toHaveLength(2);
    await change({ op: 'set-param', name: 'PostButton.radius', value: '8px' });
    await change({ op: 'set-param', name: 'PostButton.radius', when, value: '12px' });
    const moved = read();
    const refused = await call({ method: 'accept', proposal: held.ok.proposal, items });
    expect(refused.status, JSON.stringify(refused)).toBe(409);
    expect(refused.error.refused).toMatchObject([{
      edit: { what: 'param', name: 'PostButton.radius', when, before: '4px', after: null },
      now: '12px', why: 'a param by that name is there again, with other values',
    }]);
    expect(read()).toBe(moved);
  });
}, 120_000);

test.each([false, true])('a removed param revert refuses a conditional without its default, keeping another edit: %s', async keepInk => {
  const after = PARAM_DESIGN.slice(0, PARAM_DESIGN.indexOf('\nparams\n')).replace('#000000', '#111111');
  await paramHistory(PARAM_DESIGN, after, async ({ call, commit, read, change }) => {
    for (const moved of [false, true]) {
      const held = await call({ method: 'revert', commit });
      expect(held.status, JSON.stringify(held)).toBe(200);
      const when = [{ axis: 'dark-style', value: 'dim' }];
      const items = held.ok.ops.filter(o => (keepInk && o.op.name === 'ink') || (o.op.name === 'PostButton.radius' && o.op.when.length)).map(o => o.index);
      expect(items).toHaveLength(keepInk ? 2 : 1);
      if (moved) await change({ op: 'set-token', name: 'edge', value: '#303030' });
      const unchanged = read();
      const refused = await call({ method: 'accept', proposal: held.ok.proposal, items });
      expect(refused.status, JSON.stringify(refused)).toBe(409);
      expect(refused.error.refused).toMatchObject([{
        file: 'app.tamber', edit: { what: 'param', name: 'PostButton.radius', when, before: '4px', after: null },
        now: null, why: expect.stringContaining('default'),
      }]);
      expect(read()).toBe(unchanged);
    }
    const held = await call({ method: 'revert', commit });
    expect(held.status, JSON.stringify(held)).toBe(200);
    const items = held.ok.ops.filter(o => o.op.name === 'PostButton.radius').map(o => o.index);
    const part = await call({ method: 'accept', proposal: held.ok.proposal, items });
    expect(part.status, JSON.stringify(part)).toBe(200);
    expect((await call({ method: 'accept', proposal: part.ok.proposal })).status).toBe(200);
    expect(read()).toBe(PARAM_DESIGN.replace('#000000', '#111111').replace('#101010', '#303030'));
  });
}, 120_000);

test("history lists a merged branch's design edit once, in the merge", () => {
  const repo = mkdtempSync(join(tmpdir(), 'exact-tamber-merge-history-'));
  const env = { ...process.env, GIT_AUTHOR_NAME: 'Ada', GIT_AUTHOR_EMAIL: 'ada@example.test', GIT_COMMITTER_NAME: 'Ada', GIT_COMMITTER_EMAIL: 'ada@example.test' };
  const g = (...args) => {
    const r = spawnSync('git', args, { cwd: repo, encoding: 'utf8', env });
    expect(r.status, r.stderr).toBe(0);
  };
  // Lines enough between the two edits that the merge is clean.
  const design = `tamber t\n\ntheme\n  ink = #111111\n${'abcdef'.split('').map((n, i) => `  ${n} = #0${i}0${i}0${i}\n`).join('')}  edge = #222222\n`;
  const commit = (text, message) => {
    writeFileSync(join(repo, 'app.tamber'), text);
    g('commit', '-q', '-am', message);
  };
  try {
    writeFileSync(join(repo, 'app.contract'), 'component App\n  view\n    text "a"\n');
    writeFileSync(join(repo, 'app.tamber'), design);
    g('init', '-q', '-b', 'main');
    g('add', '-A');
    g('commit', '-q', '-m', 'A');
    g('checkout', '-q', '-b', 'side');
    commit(design.replace('#222222', '#bbbbbb'), 'B');
    g('checkout', '-q', 'main');
    commit(design.replace('#111111', '#aaaaaa'), 'C');
    g('merge', '-q', '--no-ff', '-m', 'M', 'side');
    const { ok } = designLog(repo, 'app.tamber');
    expect(ok.commits.map(c => c.message)).toEqual(['M', 'C', 'A']);
    expect(ok.commits[0].files[0].diff).toContain('+  edge = #bbbbbb');
  } finally {
    rmSync(repo, { recursive: true, force: true });
  }
});

test('history reads three design commits as token edits; a revert applies, is recomputed on moved files, and is refused', async () => {
  const V1 = 'tamber t\n\ntheme\n  ink = #000000\n  edge = #101010\n';
  const V2 = V1.replace('#101010', '#202020');
  const V3 = V2.replace('#000000', '#111111') + '  card = #303030\n';
  const repo = mkdtempSync(join(tmpdir(), 'exact-tamber-history-'));
  const ada = { GIT_AUTHOR_NAME: 'Ada', GIT_AUTHOR_EMAIL: 'ada@example.test', GIT_COMMITTER_NAME: 'Ada', GIT_COMMITTER_EMAIL: 'ada@example.test' };
  const g = (...args) => spawnSync('git', args, { cwd: repo, encoding: 'utf8', env: { ...process.env, ...ada } });
  writeFileSync(join(repo, 'app.contract'), 'component App\n  view\n    text "a"\n');
  g('init', '-q');
  for (const [text, message] of [[V1, 'a design'], [V2, 'a darker edge'], [V3, 'ink and a card']]) {
    writeFileSync(join(repo, 'app.tamber'), text);
    g('add', '.'); g('commit', '-q', '-m', message);
  }
  // A commit that touches no design file is not the design's history.
  writeFileSync(join(repo, 'notes.txt'), 'n\n'); g('add', '.'); g('commit', '-q', '-m', 'notes');
  const sha = rev => g('rev-parse', rev).stdout.trim();
  const listener = createServer();
  await new Promise(ok => listener.listen(0, '127.0.0.1', ok));
  const port = listener.address().port;
  const gate = developmentGate([{ origin: `http://127.0.0.1:${port}` }], port);
  const wire = tamberServer({ appDir: repo, log: () => {}, settle: 250 });
  listener.on('request', async (req, res) => {
    if (!(await wire.handle(req, res, new URL(req.url, 'http://x'), gate.check(req)))) { res.writeHead(404); res.end(); }
  });
  try {
    await wire.ready;
    const at = `http://127.0.0.1:${port}`;
    const { token } = await (await fetch(`${at}/__tamber/token`)).json();
    const call = async request => {
      const res = await fetch(`${at}/__tamber`, { method: 'POST', headers: { 'x-exact-tamber-token': token }, body: JSON.stringify(request) });
      return { status: res.status, ...(await res.json()) };
    };
    const log = await call({ method: 'history' });
    expect(log.status, JSON.stringify(log)).toBe(200);
    expect(log.ok.commits.map(c => c.message.split('\n')[0])).toEqual(['ink and a card', 'a darker edge', 'a design']);
    expect(log.ok.more).toBe(false);
    const [third, second, first] = log.ok.commits;
    expect(second).toMatchObject({ commit: sha('HEAD~2'), author: 'Ada', email: 'ada@example.test' });
    expect(second.files).toHaveLength(1);
    expect(second.files[0]).toMatchObject({ file: 'app.tamber', status: 'M', edits: [{ what: 'token', name: 'edge', when: [], before: '#101010', after: '#202020' }] });
    expect(second.files[0].diff).toContain('+  edge = #202020');
    expect(third.files[0].edits.map(e => [e.name, e.before, e.after])).toEqual([['ink', '#000000', '#111111'], ['card', null, '#303030']]);
    // Its app.contract has no state part: not the design's (stage 8 D).
    expect(first.files.map(f => f.file)).toEqual(['app.tamber']);
    expect(first.files[0].status).toBe('A');
    expect(first.files[0].edits.map(e => [e.what, e.name])).toEqual([['other', 'name'], ['token', 'ink'], ['token', 'edge']]);
    const page = await call({ method: 'history', limit: 1, skip: 1 });
    expect(page.ok.commits.map(c => c.message.split('\n')[0])).toEqual(['a darker edge']);
    expect(page.ok.more).toBe(true);
    expect((await call({ method: 'history', limit: 0 })).status).toBe(400);
    // Refused: the file moved on the commit's own edit.
    const digest = () => createHash('sha256').update(readFileSync(join(repo, 'app.tamber'))).digest('hex');
    const moved = await call({ method: 'transaction', client: 'agent', files: { 'app.tamber': digest() }, ops: [{ op: 'set-token', name: 'edge', value: '#2a2a2a' }] });
    expect(moved.status).toBe(200);
    expect((await call({ method: 'revert', commit: second.commit })).status).toBe(400);
    const refused = await call({ method: 'revert', client: 'design-tool:ada', commit: second.commit });
    expect(refused.status).toBe(409);
    expect(refused.error.message).toContain('edge');
    expect(refused.error.refused).toEqual([{ file: 'app.tamber', edit: { what: 'token', name: 'edge', when: [], before: '#101010', after: '#202020' }, now: '#2a2a2a', why: 'changed since' }]);
    // Recomputed: the third commit reverted on a file moved since, the later edit kept.
    const held = await call({ method: 'revert', client: 'design-tool:ada', commit: third.commit });
    expect(held.status, JSON.stringify(held)).toBe(200);
    expect(held.ok.review.revert).toMatchObject({ commit: third.commit, subject: 'ink and a card', moved: ['app.tamber'] });
    expect(held.ok.ops.map(o => o.op.op)).toEqual(['set-token', 'remove-token']);
    expect(readFileSync(join(repo, 'app.tamber'), 'utf8')).toContain('card');
    expect((await call({ method: 'accept', client: 'design-tool:ada', proposal: held.ok.proposal })).status).toBe(200);
    expect(readFileSync(join(repo, 'app.tamber'), 'utf8')).toBe(V1.replace('#101010', '#2a2a2a'));
    // Applied: the edit put back as the commit left it, then reverted.
    await call({ method: 'transaction', client: 'agent', files: { 'app.tamber': digest() }, ops: [{ op: 'set-token', name: 'edge', value: '#202020' }] });
    const again = await call({ method: 'revert', client: 'design-tool:ada', commit: second.commit });
    expect(again.status, JSON.stringify(again)).toBe(200);
    expect((await call({ method: 'accept', client: 'design-tool:ada', proposal: again.ok.proposal })).status).toBe(200);
    expect(readFileSync(join(repo, 'app.tamber'), 'utf8')).toBe(V1);
    const nothing = await call({ method: 'revert', client: 'design-tool:ada', commit: second.commit });
    expect(nothing.status).toBe(409);
    expect(nothing.error.message).toContain('nothing to revert');
    expect((await call({ method: 'revert', client: 'design-tool:ada', commit: sha('HEAD') })).error.message).toContain('changed no design file');
    // Held, then the files move: on the revert's own edit, the accept is
    // refused; elsewhere, it is planned again from the commit.
    const setEdge = value => call({ method: 'transaction', client: 'agent', files: { 'app.tamber': digest() }, ops: [{ op: 'set-token', name: 'edge', value }] });
    await setEdge('#202020');
    const h1 = await call({ method: 'revert', client: 'design-tool:ada', commit: second.commit });
    await setEdge('#303030');
    const stale = await call({ method: 'accept', client: 'design-tool:ada', proposal: h1.ok.proposal });
    expect(stale.status).toBe(409);
    expect(stale.error.refused[0]).toMatchObject({ edit: { name: 'edge' }, now: '#303030' });
    expect(readFileSync(join(repo, 'app.tamber'), 'utf8')).toContain('edge = #303030');
    await setEdge('#202020');
    const h2 = await call({ method: 'revert', client: 'design-tool:ada', commit: second.commit });
    await call({ method: 'transaction', client: 'agent', files: { 'app.tamber': digest() }, ops: [{ op: 'set-token', name: 'ink', value: '#121212' }] });
    const replanned = await call({ method: 'accept', client: 'design-tool:ada', proposal: h2.ok.proposal });
    expect(replanned.ok?.recomputed, JSON.stringify(replanned)).toBe(true);
    expect(replanned.ok.ops).toEqual([{ index: 0, op: { op: 'set-token', file: 'app.tamber', name: 'edge', when: [], value: '#101010', comment: null } }]);
    expect((await call({ method: 'accept', client: 'design-tool:ada', proposal: replanned.ok.proposal })).status).toBe(200);
    expect(readFileSync(join(repo, 'app.tamber'), 'utf8')).toBe(V1.replace('#000000', '#121212'));
    expect((await call({ method: 'revert', client: 'design-tool:ada', commit: '--all' })).status).toBe(400);
    expect((await call({ method: 'revert', client: 'design-tool:ada', commit: 'f'.repeat(40) })).status).toBe(404);
    // An import the tool starts from a file's text, held with its review.
    const css = readFileSync(resolve(ROOT, 'tamber/tests/import/shadcn-neutral.css'), 'utf8');
    expect((await call({ method: 'import', kind: 'shadcn', source: css })).status).toBe(400);
    const imported = await call({ method: 'import', client: 'design-tool:ada', kind: 'shadcn', source: css, name: 'globals.css' });
    expect(imported.status, JSON.stringify(imported)).toBe(200);
    expect(imported.ok.client).toBe('design-tool:ada');
    // 23 colours, `--radius` a length and its derived `lg` (LLP 1121 stage 6 H).
    expect(imported.ok.ops).toHaveLength(30);
    expect(imported.ok.review).toMatchObject({ importer: 'shadcn', counted: { source: 47, mapped: 47, reported: 0 }, tokens: 25 });
    expect(imported.ok.review.offers.map(o => o.role)).toEqual(['-exact-background', '-exact-label', '-exact-secondary-background', '-exact-separator', 'AccentColor']);
    expect(readFileSync(join(repo, 'app.tamber'), 'utf8')).toBe(V1.replace('#000000', '#121212'));
    // A stylesheet's importer takes one sheet, never silently the first of two.
    const two = await call({ method: 'import', client: 'design-tool:ada', kind: 'shadcn', sources: [{ name: 'a.css', text: css }, { name: 'b.css', text: css }] });
    expect(two.status).toBe(400);
    expect(two.error.message).toContain('imports one stylesheet');
    // Several DTCG files at once: a Figma export's modes, named as picked.
    const figma = resolve(ROOT, 'tamber/tests/import/figma');
    const sources = readdirSync(figma).map(name => ({ name, text: readFileSync(join(figma, name), 'utf8') }));
    const modes = await call({ method: 'import', client: 'design-tool:ada', kind: 'dtcg', sources });
    expect(modes.status, JSON.stringify(modes).slice(0, 400)).toBe(200);
    expect(modes.ok.review.counted).toEqual({ source: 104, mapped: 101, reported: 3 });
    // A design file whose name git quotes still has its diff.
    writeFileSync(join(repo, 'ünï.tamber'), 'tamber u\n\ntheme\n  u.ink = #010101\n');
    g('add', 'ünï.tamber'); g('commit', '-q', '-m', 'a named file');
    const named = await call({ method: 'history', limit: 1 });
    expect(named.ok.commits[0].files).toHaveLength(1);
    expect(named.ok.commits[0].files[0]).toMatchObject({ file: 'ünï.tamber', status: 'A' });
    expect(named.ok.commits[0].files[0].diff).toContain('+  u.ink = #010101');
    const astryx = await call({ method: 'import', client: 'design-tool:ada', kind: 'astryx', source: '{}' });
    expect(astryx.status).toBe(400);
    expect(astryx.error.message).toContain('bun exact.mjs tamber import astryx');
  } finally {
    await wire.close();
    listener.close();
    rmSync(repo, { recursive: true, force: true });
  }
}, 600_000);

test("a held revert keeps its untouched files' digests and a person's selection when it is planned again", async () => {
  const A1 = 'tamber t\n\ntheme\n  ink = #000000\n  edge = #101010\n';
  const B1 = 'tamber b\n\ntheme\n  b.ink = #010101\n';
  const repo = mkdtempSync(join(tmpdir(), 'exact-tamber-revert-'));
  const env = { ...process.env, GIT_AUTHOR_NAME: 'Ada', GIT_AUTHOR_EMAIL: 'ada@example.test', GIT_COMMITTER_NAME: 'Ada', GIT_COMMITTER_EMAIL: 'ada@example.test' };
  const g = (...args) => spawnSync('git', args, { cwd: repo, encoding: 'utf8', env });
  writeFileSync(join(repo, 'app.contract'), 'component App\n  view\n    text "a"\n');
  g('init', '-q');
  for (const [a, b] of [[A1, B1], [A1.replace('#000000', '#222222').replace('#101010', '#202020'), B1.replace('#010101', '#020202')]]) {
    writeFileSync(join(repo, 'app.tamber'), a); writeFileSync(join(repo, 'b.tamber'), b);
    g('add', '.'); g('commit', '-q', '-m', 'design');
  }
  const commit = g('rev-parse', 'HEAD').stdout.trim();
  const listener = createServer();
  await new Promise(ok => listener.listen(0, '127.0.0.1', ok));
  const port = listener.address().port;
  const gate = developmentGate([{ origin: `http://127.0.0.1:${port}` }], port);
  const wire = tamberServer({ appDir: repo, log: () => {}, settle: 250 });
  listener.on('request', async (req, res) => {
    if (!(await wire.handle(req, res, new URL(req.url, 'http://x'), gate.check(req)))) { res.writeHead(404); res.end(); }
  });
  try {
    await wire.ready;
    const at = `http://127.0.0.1:${port}`;
    const { token } = await (await fetch(`${at}/__tamber/token`)).json();
    const call = async request => {
      const res = await fetch(`${at}/__tamber`, { method: 'POST', headers: { 'x-exact-tamber-token': token }, body: JSON.stringify(request) });
      return { status: res.status, ...(await res.json()) };
    };
    const digest = f => createHash('sha256').update(readFileSync(join(repo, f))).digest('hex');
    const set = (file, name, value) => call({ method: 'transaction', client: 'agent', files: { [file]: digest(file) }, ops: [{ op: 'set-token', file, name, value }] });
    // b.tamber already as before: no operation there, but its digest guards the plan.
    await set('b.tamber', 'b.ink', '#010101');
    const held = await call({ method: 'revert', client: 'ada', commit });
    expect(held.ok.ops.map(o => o.op.file)).toEqual(['app.tamber', 'app.tamber']);
    await set('b.tamber', 'b.ink', '#030303');
    const stale = await call({ method: 'accept', client: 'ada', proposal: held.ok.proposal });
    expect(stale.status, JSON.stringify(stale)).toBe(409);
    expect(stale.error.refused[0]).toMatchObject({ file: 'b.tamber', edit: { name: 'b.ink' }, now: '#030303' });
    expect(readFileSync(join(repo, 'app.tamber'), 'utf8')).toContain('ink = #222222');
    // Only `ink` kept; then the file moves on the item left out: planned
    // again, it is still only `ink`.
    await set('b.tamber', 'b.ink', '#010101');
    const two = await call({ method: 'revert', client: 'ada', commit });
    const inkAt = two.ok.ops.findIndex(o => o.op.name === 'ink');
    const part = await call({ method: 'accept', client: 'ada', proposal: two.ok.proposal, items: [inkAt] });
    expect(part.ok.ops.map(o => o.op.name)).toEqual(['ink']);
    await set('app.tamber', 'edge', '#2b2b2b');
    const again = await call({ method: 'accept', client: 'ada', proposal: part.ok.proposal });
    expect(again.ok?.recomputed, JSON.stringify(again)).toBe(true);
    expect(again.ok.ops.map(o => o.op.name)).toEqual(['ink']);
    expect((await call({ method: 'accept', client: 'ada', proposal: again.ok.proposal })).status).toBe(200);
    expect(readFileSync(join(repo, 'app.tamber'), 'utf8')).toBe(A1.replace('#101010', '#2b2b2b'));
    // An axis's values are each their own item: only `dim` kept stays only `dim`.
    const AX = v => `tamber b\n\naxes\n  tone = ${v}\n\ntheme\n  b.ink = #010101\n`;
    for (const v of ['black | dim | navy', 'black']) { writeFileSync(join(repo, 'b.tamber'), AX(v)); g('add', 'b.tamber'); g('commit', '-q', '-m', `tone ${v}`); }
    // The server reads the checkout.
    for (let i = 0; i < 100 && !(await call({ method: 'hello' })).ok.files['b.tamber']?.startsWith(digest('b.tamber').slice(0, 8)); i++) await Bun.sleep(50);
    const axes = await call({ method: 'revert', client: 'ada', commit: g('rev-parse', 'HEAD').stdout.trim() });
    expect(axes.ok.ops.map(o => o.op.value)).toEqual(['dim', 'navy']);
    const dimOnly = await call({ method: 'accept', client: 'ada', proposal: axes.ok.proposal, items: [0] });
    await set('b.tamber', 'b.ink', '#040404');
    const again2 = await call({ method: 'accept', client: 'ada', proposal: dimOnly.ok.proposal });
    expect(again2.ok?.ops.map(o => o.op.value), JSON.stringify(again2)).toEqual(['dim']);
    // A client's own review cannot pose as a revert.
    expect((await call({ method: 'propose', hold: true, client: 'agent', ops: [{ op: 'set-token', name: 'ink', value: '#090909' }], review: { revert: { commit, only: {} } } })).status).toBe(400);
  } finally {
    await wire.close();
    listener.close();
    rmSync(repo, { recursive: true, force: true });
  }
}, 600_000);

// The app's own .contract files are in history and revert too (stage 8 D's
// follow-up): a commit's state rows are edits, its other lines `code`,
// which a revert leaves alone.
test("history reads a .contract commit's state rows, and a revert undoes them, leaving its code", async () => {
  const V1 = 'style Post\n  background-color="#0a7aff"\n\ncomponent App\n  view\n    column class=Post\n      text "Post"\n';
  const V2 = 'style Post\n  background-color="#0a7aff"\n  :active\n    opacity=0.5\n\ncomponent App\n  view\n    column class=Post\n      text "Posted"\n';
  const repo = mkdtempSync(join(tmpdir(), 'exact-tamber-history-contract-'));
  const env = { ...process.env, GIT_AUTHOR_NAME: 'Ada', GIT_AUTHOR_EMAIL: 'ada@example.test', GIT_COMMITTER_NAME: 'Ada', GIT_COMMITTER_EMAIL: 'ada@example.test' };
  const g = (...args) => spawnSync('git', args, { cwd: repo, encoding: 'utf8', env });
  writeFileSync(join(repo, 'app.tamber'), 'tamber t\n\ntheme\n  ink = #000000\n');
  writeFileSync(join(repo, 'app.contract'), V1);
  g('init', '-q'); g('add', '.'); g('commit', '-q', '-m', 'app');
  writeFileSync(join(repo, 'app.contract'), V2); g('add', '.'); g('commit', '-q', '-m', 'a pressed post');
  // A commit that changes only a Contract source's code is no design commit.
  writeFileSync(join(repo, 'app.contract'), V2.replace('Posted', 'Sent')); g('add', '.'); g('commit', '-q', '-m', 'reword');
  const listener = createServer();
  await new Promise(ok => listener.listen(0, '127.0.0.1', ok));
  const port = listener.address().port;
  const gate = developmentGate([{ origin: `http://127.0.0.1:${port}` }], port);
  const wire = tamberServer({ appDir: repo, log: () => {}, settle: 250 });
  listener.on('request', async (req, res) => {
    if (!(await wire.handle(req, res, new URL(req.url, 'http://x'), gate.check(req)))) { res.writeHead(404); res.end(); }
  });
  try {
    await wire.ready;
    const at = `http://127.0.0.1:${port}`;
    const { token } = await (await fetch(`${at}/__tamber/token`)).json();
    const call = async request => {
      const res = await fetch(`${at}/__tamber`, { method: 'POST', headers: { 'x-exact-tamber-token': token }, body: JSON.stringify(request) });
      return { status: res.status, ...(await res.json()) };
    };
    await call({ method: 'hello', client: 'test' });
    const log = await call({ method: 'history' });
    expect(log.ok.commits.map(c => c.message.split('\n')[0])).toEqual(['a pressed post', 'app']);
    expect(log.status, JSON.stringify(log)).toBe(200);
    const pressed = log.ok.commits.find(c => c.message.startsWith('a pressed post'));
    const file = pressed.files.find(f => f.file === 'app.contract');
    expect(file.edits).toEqual([{ what: 'state', name: 'Post', state: 'active', row: 'opacity', when: [], before: null, after: '0.5' }]);
    expect(file.code).toBe(true);
    const held = await call({ method: 'revert', client: 'person', commit: pressed.commit });
    expect(held.status, JSON.stringify(held)).toBe(200);
    expect(held.ok.ops).toEqual([{ index: 0, op: { op: 'unset-state', style: 'Post', state: 'active', row: 'opacity', file: 'app.contract' } }]);
    expect(held.ok.review.revert.code).toEqual(['app.contract']);
    const taken = await call({ method: 'accept', client: 'person', proposal: held.ok.proposal });
    expect(taken.status, JSON.stringify(taken)).toBe(200);
    const now = readFileSync(join(repo, 'app.contract'), 'utf8');
    expect(now).not.toContain(':active');
    expect(now).toContain('text "Sent"');
    // Again: nothing left to revert.
    expect((await call({ method: 'revert', client: 'person', commit: pressed.commit })).status).toBe(409);
  } finally {
    await wire.close();
    listener.close();
    rmSync(repo, { recursive: true, force: true });
  }
}, 600_000);
