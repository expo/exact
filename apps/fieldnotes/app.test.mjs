import { expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { checkOp, ratio } from '../../scripts/agent-check.mjs';
import schema from '../../kernel/tables/schema.json';

test('green button ink clears 4.5:1 and system-red restore ink clears 3:1', () => {
  const lines = readFileSync(resolve(import.meta.dir, 'app.contract'), 'utf8').split('\n');
  const attribute = (line, name) => line.match(new RegExp(`(?:^| )${name}="([^"]+)"`))?.[1];
  const accent = attribute(lines.find(line => line.includes('testId="fieldnotes"')), 'accent-color');
  const failures = [];
  for (const id of ['new-note', 'save-note', 'save-backup', 'confirm-restore']) {
    const start = lines.findIndex(line => line.includes(`testId="${id}"`));
    expect(start).toBeGreaterThan(-1);
    const button = lines[start], indent = button.search(/\S/);
    const children = lines.slice(start + 1).slice(0, lines.slice(start + 1)
      .findIndex(line => line.trim() && line.search(/\S/) <= indent));
    const inks = children.flatMap(line => [...line.matchAll(/ (?:color|-exact-tint-color)="([^"]+)"/g)].map(m => m[1]));
    expect(inks.length).toBeGreaterThan(0);
    if (id === 'confirm-restore') {
      expect(attribute(button, 'accent-color')).toBe('-exact-system-red');
      expect(attribute(button, 'background-color')).toBe('AccentColor');
      expect(inks).toEqual(['AccentColorText']);
    }
    for (const [scheme, column] of [['light', 4], ['dark', 5]]) {
      const colour = value => {
        if (value === 'AccentColor') return colour(attribute(button, 'accent-color') ?? accent);
        const pair = value.match(/^light-dark\(([^,]+),\s*([^)]+)\)$/);
        if (pair) return colour(pair[scheme === 'light' ? 1 : 2].trim());
        if (!value.startsWith('#')) value = schema.colors.find(row => row[0] === value.replace(/^-exact-/, ''))?.[column];
        expect(value).toMatch(/^#[0-9a-f]{6}$/i);
        return [1, 3, 5].map(offset => parseInt(value.slice(offset, offset + 2), 16));
      };
      for (const ink of inks) {
        const contrast = ratio(colour(ink), colour(attribute(button, 'background-color')));
        if (contrast < (id === 'confirm-restore' ? 3 : 4.5)) failures.push({ id, scheme, ink, contrast });
      }
    }
  }
  expect(failures).toEqual([]);
});

// Hold one library open so the search race is observable even with a fast worker.
async function withHeldLibraryRead(session, run) {
  await session.carrier.evaluate(`(() => {
    const send = Worker.prototype.postMessage;
    let held;
    globalThis.releaseFieldnotesRead = () => {
      Worker.prototype.postMessage = send;
      delete globalThis.releaseFieldnotesRead;
      if (held) send.apply(held[0], held[1]);
    };
    Worker.prototype.postMessage = function(...args) {
      const request = args[0];
      if (!held && request?.op === 'open' && request.args?.[0] === 'app:/data/fieldnotes.db') held = [this, args];
      else send.apply(this, args);
    };
  })()`);
  const release = () => session.carrier.evaluate('globalThis.releaseFieldnotesRead?.()');
  try {
    return await run(release);
  } finally {
    await release();
  }
}

// Build first with `bun host/web/build.mjs fieldnotes`.
test('Fieldnotes text remains readable in both schemes', async () => {
  const cache = mkdtempSync(resolve(import.meta.dir, '.contrast-'));
  let session;
  try {
    session = await open({
      host: 'web', app: 'fieldnotes', size: [1100, 900], storage: 'contrast',
      env: { XDG_CACHE_HOME: cache, EXACT_AGENT_STORAGE_FRESH: '1', EXACT_AGENT_EPHEMERAL_PORT: '1' },
    });
    await session.carrier.evaluate('globalThis.originalFieldnotesSend = Worker.prototype.postMessage');
    await expect(withHeldLibraryRead(session, async () => {
      throw new Error('injected failure before settlement');
    })).rejects.toThrow('injected failure before settlement');
    expect(await session.carrier.evaluate('Worker.prototype.postMessage === originalFieldnotesSend')).toBe(true);
    expect(await session.carrier.evaluate('typeof globalThis.releaseFieldnotesRead')).toBe('undefined');
    const failures = [];
    const check = async (screen, target) => {
      await session.clock('settle');
      await session.tree(target);
      const result = await checkOp(session, ['both']);
      expect(result.check.map(report => report.scheme)).toEqual(['light', 'dark']);
      for (const report of result.check) {
        failures.push(...report.findings.filter(f => f.kind === 'contrast')
          .map(f => ({ screen, scheme: report.scheme, ...f })));
      }
    };
    await check('empty notebook', 'empty-notebook');
    await session.type('note-title', 'A walk in the woods');
    await session.type('note-body', 'Ferns beside the path.');
    await session.tap('pin-note');
    await session.tap('save-note');
    await session.clock('settle');
    const saved = (await session.state()).derives.editingId;
    expect(saved).not.toBe('');
    await session.tap('new-note');
    await session.clock('settle');
    await session.tap(`note-${saved}`);
    await session.clock('settle');
    await session.type('note-body', 'Ferns beside the path. Remember the clearing.');
    await check('editor with draft', 'unsaved');
    await session.tap('find-notes');
    await session.type('search', 'woods');
    await check('search', `note-${saved}`);
    await session.tap('discard');
    await withHeldLibraryRead(session, async release => {
      await session.tap('close-search');
      expect((await session.state()).pending.map(request => request.name)).toContain('library');
      await Promise.all([session.clock('settle'), release()]);
      const settled = await session.state();
      expect(settled.pending.map(request => request.name)).not.toContain('library');
      expect(settled.derives.busy).toBe(false);
    });
    await session.tap('delete-note');
    await check('delete confirmation', 'confirm-delete');
    await session.tap('cancel-delete');
    await session.tap('backups');
    await check('backups', 'backup-panel');
    await session.tap('save-backup');
    await check('saved backup', 'backup-text');
    await session.tap('restore-backup');
    await check('restore confirmation', 'confirm-restore');
    await session.tap('cancel-restore');
    await session.type('import-text', 'not a backup');
    await session.tap('restore-backup');
    await session.tap('confirm-restore');
    await session.clock('settle');
    expect((await session.state()).slots.status.failed).toBe(true);
    await check('invalid backup', 'status-message');
    expect(failures).toEqual([]);
  } finally {
    await session?.close();
    rmSync(cache, { recursive: true, force: true });
  }
}, 30000);

test('the JS target backs up through Rust and restores through TypeScript', async () => {
  let session;
  try {
    session = await open({ host: 'web', app: 'fieldnotes', storage: 'rust-backup',
      env: { EXACT_AGENT_STORAGE_FRESH: '1', EXACT_AGENT_EPHEMERAL_PORT: '1' } });
    await session.clock('settle');
    await session.carrier.evaluate(`(() => {
      const answer = exact.data.answer, fetch = exact.data.fetch;
      globalThis.backupCalls = [];
      exact.data.answer = function(name, ...args) {
        const result = answer.call(this, name, ...args);
        backupCalls.push({ name, rust: !!result?.req?.storage, typescript: !!result?.promise });
        return result;
      };
      if (fetch) exact.data.fetch = function(request, ...args) {
        if (globalThis.failBackup && request.storage) {
          globalThis.failBackup = false;
          return Promise.resolve({ storage: new TextEncoder().encode(JSON.stringify({ error: 'Backup disk full.' })) });
        }
        return fetch.call(this, request, ...args);
      };
    })()`);
    await session.type('note-title', 'Field 🌿 東京');
    await session.type('note-body', 'Café, naïve, 🦀\nA second line.');
    await session.tap('save-note');
    await session.clock('settle');
    await session.tap('backups');
    await session.tap('save-backup');
    await session.clock('settle');
    expect(await session.carrier.evaluate('backupCalls.find(c => c.name === "backupNotes")'))
      .toEqual({ name: 'backupNotes', rust: true, typescript: false });
    let state = await session.state();
    expect(state.slots.status).toMatchObject({ revision: 2, message: 'Backup saved with 1 notes.', failed: false });
    const backup = JSON.parse(state.slots.status.backupText);
    expect(backup.notes).toEqual([{ id: state.derives.editingId, title: 'Field 🌿 東京',
      body: 'Café, naïve, 🦀\nA second line.', pinned: false }]);
    await session.type('import-text', JSON.stringify({ version: 1, notes: [] }));
    await session.tap('restore-backup');
    await session.tap('confirm-restore');
    await session.clock('settle');
    expect((await session.state()).slots.status).toMatchObject({ message: 'Restored 0 notes.', failed: false });
    await session.type('import-text', '');
    await session.tap('restore-backup');
    await session.tap('confirm-restore');
    await session.clock('settle');
    state = await session.state();
    expect(state.slots.status).toMatchObject({ message: 'Restored 1 notes.', failed: false });
    expect(await session.carrier.evaluate('backupCalls.filter(c => ["saveNote", "restoreNotes"].includes(c.name))')).toEqual([
      { name: 'saveNote', rust: false, typescript: true },
      { name: 'restoreNotes', rust: false, typescript: true },
      { name: 'restoreNotes', rust: false, typescript: true },
    ]);
    const denial = await session.carrier.evaluate(`(async () => {
      const result = await exact.data.fetch({ storage: JSON.stringify({ version: 1, op: 'fs.readFile', args: { path: 'app:/tmp/shared/no.txt' } }) });
      return JSON.parse(new TextDecoder().decode(result.storage));
    })()`);
    expect(denial.error).toContain('denied');
    await session.carrier.evaluate('globalThis.failBackup = true');
    await session.tap('save-backup');
    await session.clock('settle');
    expect((await session.state()).slots.status).toMatchObject({ message: 'Backup disk full.', failed: true, backupText: '' });
    await session.tap('save-backup');
    await session.clock('settle');
    expect((await session.state()).slots.status).toMatchObject({ revision: 6, message: 'Backup saved with 1 notes.', failed: false });
  } finally { await session?.close(); }
}, 30000);
