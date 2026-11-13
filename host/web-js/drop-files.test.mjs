import { test, expect } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { reloadWeb } from '../../scripts/agent-test.mjs';
import { webCompiler } from './module.mjs';
import { need } from '../../scripts/coverage.mjs';

// Files dragged or pasted in (LLP 1069.010, "Files dropped or pasted"): any
// type with no `file_handlers`, a folder as a folder's handle, a drag over a
// textarea landing on the column's `drop`, `dragenter`/`dragleave` around
// it (a drop sends no `dragleave`, as the DOM's), and a paste's files. The drags are Chrome's own (`Input.dispatchDragEvent`).
test('a column hears a file drag over its textarea, its drop and a paste of files', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/drop-files-test-'));
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Drop Files Test', app: { id: 'test.drop-files', name: 'Drop Files Test' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `component DropFiles
  state over = false
  state enters = 0
  state leaves = 0
  state got = ""
  state pasted = ""
  action entered(e: DragEvent)
    over = true
    enters = enters + 1
  action left
    over = false
    leaves = leaves + 1
  action dropped(e: DragEvent)
    got = e.files.join(" ")
  action pastedIn(e: ClipboardEvent)
    if e.files.length > 0
      pasted = e.files.join(" ")
      preventDefault()
  view
    column testId="column" width=400 height=400 padding=16 dragenter=entered dragleave=left drop=dropped
      textarea testId="composer" paste=pastedIn
      text "label" testId="label"
`);
    const files = resolve(dir, 'files');
    mkdirSync(resolve(files, 'folder'), { recursive: true });
    writeFileSync(resolve(files, 'notes.xyz'), 'not a declared type');
    writeFileSync(resolve(files, 'folder', 'a.md'), 'a');
    writeFileSync(resolve(files, 'shot.png'), Buffer.from('89504e470d0a1a0a', 'hex'));
    const dist = resolve(dir, 'dist');
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'drop-files-test', '--out', dist, '--render', 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname;
      if (path.startsWith('/shared/')) return new Response(Bun.file(resolve(root, 'host/web', path.slice('/shared/'.length))));
      const file = resolve(dist, path === '/' ? 'index.html' : path.slice(1));
      return existsSync(file) ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
    } });
    browser = await open({ host: 'web', url: server.url.href, size: [800, 800] });
    const slots = async () => (await browser.state()).slots;
    // Over the column, then into its textarea: one enter, no leave between.
    await browser.tap('column', { dragenter: [resolve(files, 'notes.xyz')] });
    await browser.tap('composer', { dragenter: [resolve(files, 'notes.xyz')] });
    expect(await slots()).toMatchObject({ over: true, enters: 1 });
    await browser.tap('column', { dragleave: true });
    expect(await slots()).toMatchObject({ over: false, leaves: 1 });
    // Dropped on the textarea: the column's, a file of no declared type and a folder; no path text, and no dragleave.
    await browser.tap('composer', { drop: [resolve(files, 'notes.xyz'), resolve(files, 'folder')] });
    await browser.clock('settle');
    expect(await slots()).toMatchObject({ over: true, enters: 2, leaves: 1, got: 'doc:/1/notes.xyz doc:/2/folder' });
    // The next drag enters afresh: the drop ended the last one.
    await browser.tap('column', { dragenter: [resolve(files, 'notes.xyz')] });
    expect(await slots()).toMatchObject({ enters: 3, leaves: 1 });
    await browser.tap('column', { dragleave: true });
    expect(await browser.carrier.evaluate('document.querySelector("[data-testid=composer]").value')).toBe('');
    // A paste of an image's data: one file, the field's own paste prevented; a paste of text alone is the field's.
    await browser.type('composer', { clipboard: 'paste', text: '', image: resolve(files, 'shot.png') });
    expect((await slots()).pasted).toBe('doc:/3/image.png');
    await browser.type('composer', { clipboard: 'paste', text: 'hello' });
    expect(await browser.carrier.evaluate('document.querySelector("[data-testid=composer]").value')).toBe('hello');
    expect((await slots()).pasted).toBe('doc:/3/image.png');
    // A reload is a new document with no gesture held: a contact down and a file drag over the page end with the old
    // one, so the next press goes down and the next drag of the same file enters.
    await browser.tap('column', { down: true });
    await browser.tap('column', { dragenter: [resolve(files, 'notes.xyz')] });
    await reloadWeb(browser);
    await browser.tap('column', { down: true });
    await browser.pointer('up');
    await browser.tap('column', { drop: [resolve(files, 'notes.xyz')] });
    await browser.clock('settle');
    expect(await slots()).toMatchObject({ enters: 1, got: 'doc:/1/notes.xyz' });
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 60000);

for (const runtime of ['JS', 'wasm']) {
  const check = need('fixture', runtime === 'wasm' && !process.env.EXACT_WEB_DIST ? 'name a wasm web build with EXACT_WEB_DIST' : null);
  check(`${runtime}: tapping a file input holds one picker for a choice or cancellation`, async () => {
    const root = resolve(import.meta.dir, '../..');
    mkdirSync(resolve(root, 'target'), { recursive: true });
    const dir = mkdtempSync(resolve(root, 'target/picker-tap-test-'));
    let browser, server;
    try {
      writeFileSync(resolve(dir, 'app.json'), JSON.stringify({
        name: 'Picker Tap',
        app: { id: runtime === 'wasm' ? JSON.parse(readFileSync(resolve(process.env.EXACT_WEB_DIST, 'exact.json'), 'utf8')).app.id : 'test.picker-tap', name: 'Picker Tap' }, host: { web: {} },
        file_handlers: [{ action: '/', accept: { 'application/pdf': ['.pdf'] } }],
      }));
      writeFileSync(resolve(dir, 'app.contract'), `component PickerTap
  state changes = 0
  state cancels = 0
  state names = ""
  state types = ""
  state paths = ""
  state allowMany = false
  state defaultMany = false
  state unavailable = false
  state blocked = false
  state present = true
  state redirectedEvents = 0
  state swapped = false
  action picked(files: Picked[])
    changes = changes + 1
    names = files.map(f => f.name).join(" ")
    types = files.map(f => f.type).join(" ")
    paths = files.map(f => f.path).join(" ")
  action cancelled
    cancels = cancels + 1
  action open
    showPicker("command")
  action openOther
    showPicker("pdf")
  action openMissing
    showPicker("missing")
  action redirected(files: Picked[])
    redirectedEvents = redirectedEvents + files.length
  action cancelRedirected
    redirectedEvents = redirectedEvents + 1
  action swapAndOpen
    swapped = !swapped
    showPicker("pdf")
  action openMany
    allowMany = true
    showPicker("ancestor")
  action allowMore
    defaultMany = true
  action disable
    unavailable = true
  action block
    blocked = true
  action remove
    present = false
  view
    column gap=8
      input type="file" id="pdf" testId="pdf" accept="application/pdf" change=picked cancel=cancelled
      input type="file" testId="anonymous" multiple=true change=picked cancel=cancelled
      input type="file" id="command" testId="command" press=open change=picked cancel=cancelled
      input type="file" testId="redirect" press=openOther change=redirected cancel=cancelRedirected
      column press=openOther
        input type="file" testId="redirect-ancestor" change=redirected cancel=cancelRedirected
      input type="file" testId="deferred" press=swapAndOpen change=redirected cancel=cancelRedirected
      if swapped
        text "After" sharedElement="marker" -exact-layout-transition="20ms linear"
      else
        text "Before" sharedElement="marker" -exact-layout-transition="20ms linear"
      input type="file" id="refused" testId="refused" press=openMissing change=picked cancel=cancelled
      input type="file" testId="prevented" change=picked
      input type="file" testId="disabled" disabled=true change=picked
      input type="file" testId="fieldset" change=picked
      column inert=true
        input type="file" testId="inert" change=picked
      input type="file" testId="hidden" visibility="hidden" change=picked
      input type="file" testId="bare"
      column press=openMany
        input type="file" id="ancestor" testId="ancestor" multiple=allowMany change=picked cancel=cancelled
      column press=allowMore
        input type="file" testId="default-many" multiple=defaultMany change=picked cancel=cancelled
      column press=disable
        input type="file" testId="disabled-by-press" disabled=unavailable
      column press=block inert=blocked
        input type="file" testId="inert-by-press"
      column press=remove
        if present
          input type="file" id="missing" testId="removed-by-press"
`);
      const pdf = resolve(dir, 'report.pdf'), other = resolve(dir, 'other.pdf'), txt = resolve(dir, 'notes.txt');
      writeFileSync(pdf, '%PDF-1.7\n'); writeFileSync(other, '%PDF-1.7\n'); writeFileSync(txt, 'notes');
      const dist = resolve(dir, 'dist');
      const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'picker-tap-test', '--out', dist, '--render', 'none'], {
        cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
      });
      const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
      expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
      server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
        const path = new URL(req.url).pathname;
        if (path === '/__plan') return new Response(Bun.file(resolve(dist, 'app.plan')));
        const base = runtime === 'wasm' ? process.env.EXACT_WEB_DIST : dist;
        const file = path.startsWith('/shared/') ? resolve(root, 'host/web', path.slice(8)) : resolve(base, path === '/' ? 'index.html' : path.slice(1));
        return existsSync(file) ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
      } });
      browser = await open({ host: 'web', url: server.url.href, ...(runtime === 'wasm' ? { plan: resolve(dist, 'app.plan') } : {}) });
      const held = async () => (await browser.state()).pending.filter(p => p.device?.capability === 'pick');
      const slots = async () => (await browser.state()).slots;
      await browser.tap('pdf');
      expect(await held()).toMatchObject([{ device: { args: { id: 'pdf', accept: ['application/pdf'], multiple: false } } }]);
      const ticket = (await held())[0].ticket;
      await expect(browser.type('@pick', txt)).rejects.toThrow('not among accept');
      await expect(browser.type('@pick', `${pdf}\n${other}\n`)).rejects.toThrow('takes one file');
      expect((await held()).map(h => h.ticket)).toEqual([ticket]);
      expect((await browser.type('@pick', pdf)).delivery).toBe('substituted');
      expect(await slots()).toMatchObject({ changes: 1, cancels: 0, names: 'report.pdf', types: 'application/pdf' });
      expect((await slots()).paths).toMatch(/^app:\/tmp\/picked\/[^/]+\.pdf$/);
      expect(await held()).toEqual([]);
      await expect(browser.tap(`@${ticket}`, { choice: 'cancel' })).rejects.toThrow('not pending');
      await browser.tap('pdf');
      await browser.tap('@pick', { choice: 'cancel' });
      expect(await slots()).toMatchObject({ changes: 1, cancels: 1, names: 'report.pdf' });
      await browser.tap('anonymous');
      expect(await held()).toHaveLength(1);
      await browser.type('@pick', `${pdf}\n${txt}\n`);
      expect(await slots()).toMatchObject({ changes: 2, names: 'report.pdf notes.txt', types: 'application/pdf text/plain' });
      await browser.tap('command');
      expect(await held()).toHaveLength(1);
      await browser.type('@pick', pdf);
      expect(await slots()).toMatchObject({ changes: 3, cancels: 1 });
      expect(await held()).toEqual([]);
      await browser.tap('bare');
      expect(await held()).toHaveLength(1);
      await browser.tap('@pick', { choice: 'cancel' });
      await browser.carrier.evaluate(`(() => {
        document.querySelector('[data-testid="prevented"]').addEventListener('click', e => e.preventDefault());
        const input = document.querySelector('[data-testid="fieldset"]'), fieldset = document.createElement('fieldset');
        fieldset.disabled = true; input.before(fieldset); fieldset.append(input);
      })()`);
      for (const target of ['prevented', 'disabled', 'fieldset', 'inert', 'hidden']) {
        await browser.tap(target).catch(error => {
          if (!/disabled|inert|hidden|not visible|no visible|no painted/.test(error.message)) throw error;
        });
        expect(await held()).toEqual([]);
      }
      await browser.tap('ancestor');
      expect(await held()).toMatchObject([{ device: { args: { id: 'ancestor', multiple: true } } }]);
      await browser.type('@pick', `${pdf}\n${other}\n`);
      expect(await slots()).toMatchObject({ changes: 4, names: 'report.pdf other.pdf' });
      expect(await held()).toEqual([]);
      await browser.tap('default-many');
      expect(await held()).toMatchObject([{ device: { args: { multiple: true } } }]);
      await browser.type('@pick', `${pdf}\n${other}\n`);
      expect(await slots()).toMatchObject({ changes: 5, names: 'report.pdf other.pdf' });
      expect(await held()).toEqual([]);
      for (const target of ['disabled-by-press', 'inert-by-press', 'removed-by-press']) {
        await browser.tap(target);
        expect(await held()).toEqual([]);
      }
      expect(await slots()).toMatchObject({ unavailable: true, blocked: true, present: false });
      for (const [i, target] of ['redirect', 'redirect-ancestor'].entries()) {
        await browser.tap(target);
        expect(await held()).toMatchObject([{ device: { args: { id: 'pdf' } } }]);
        await browser.type('@pick', pdf);
        expect(await held()).toEqual([]);
        expect(await slots()).toMatchObject({ changes: 6 + i, cancels: 1 + i, redirectedEvents: 0 });
        await browser.tap(target);
        expect(await held()).toMatchObject([{ device: { args: { id: 'pdf' } } }]);
        await browser.tap('@pick', { choice: 'cancel' });
        expect(await held()).toEqual([]);
        expect(await slots()).toMatchObject({ changes: 6 + i, cancels: 2 + i, redirectedEvents: 0 });
      }
      await browser.tap('pdf');
      await browser.tap('bare');
      expect((await held()).map(h => h.name)).toEqual(['pdf', 'bare']);
      await browser.tap('@bare', { choice: 'cancel' });
      await browser.tap('@pdf', { choice: 'cancel' });
      await browser.tap('refused');
      expect(await held()).toMatchObject([{ device: { args: { id: 'refused' } } }]);
      await browser.type('@pick', pdf);
      expect(await held()).toEqual([]);
      expect(await slots()).toMatchObject({ changes: 8, cancels: 4, redirectedEvents: 0 });
      // The JS executor runs a press that hands on a shared element's name inside a view transition (shared.js),
      // after this click: the press's showPicker is the one held.
      await browser.tap('deferred');
      expect(await held()).toMatchObject([{ device: { args: { id: 'pdf' } } }]);
      await browser.tap('@pick', { choice: 'cancel' });
      expect(await held()).toEqual([]);
      expect(await slots()).toMatchObject({ swapped: true, changes: 8, cancels: 5, redirectedEvents: 0 });
    } finally {
      await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
    }
  }, 60000);
}
