import { hostFixtureFile as readFileSync } from './glue-fixture.mjs';
// @ref LLP 1045 D1/D5/D6 — native DOM edits, controlled source and shared undo.
import { afterAll, test, expect } from 'bun:test';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { chromium as installed, suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { buildEditor } from '../../web-js/module.mjs';
import { browserMissing } from '../../../scripts/agent-playwright.mjs';
import { need, notApplicable } from '../../../scripts/coverage.mjs';
import { fileURLToPath } from 'node:url';

const WEB = resolve(fileURLToPath(new URL('..', import.meta.url)));
const chrome = installed();
const engines = ['chromium', 'firefox', 'webkit'];
const missing = Object.fromEntries(await Promise.all(engines.map(async e => [e, await browserMissing(e)])));
const engineTest = Object.fromEntries(engines.map(e => [e, need(e === 'chromium' ? 'chrome' : e, missing[e])]));
const wasm = engines.some(e => !missing[e]) ? readFileSync(await buildEditor()) : null;

/** `attrs`: the textarea's own attributes beside its markup, as a page writes them before the editor mounts. */
async function editor(engine, source, drive, runtime = false, attrs = '') {
  const server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname;
    if (['/markup-editor.js', '/navigation.js', '/atoms.js', '/geometry-glue.js'].includes(path)) return new Response(readFileSync(resolve(WEB, path.slice(1))), { headers: { 'content-type': 'text/javascript' } });
    if (path === '/markup-editor.wasm') return new Response(wasm, { headers: { 'content-type': 'application/wasm' } });
    if (path === '/names.js') return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } }); // no data sources in this fixture
    if (runtime && path.endsWith('.js')) {
      // The JS target's modules over the host's glue; a module that
      // re-exports the host's own reaches it through `../web/`.
      const js = path.startsWith('/web/') ? resolve(WEB, path.slice(5)) : resolve(WEB, '../web-js', path.slice(1));
      return new Response(readFileSync(existsSync(js) ? js : resolve(WEB, path.slice(1))), { headers: { 'content-type': 'text/javascript' } });
    }
    return new Response(`<!doctype html><meta charset="utf-8"><textarea id="editor" markup="markdown" ${attrs} style="width:500px;height:240px"></textarea><pre id="echo"></pre>
      <script type="module">
        globalThis.exact = {};
        let field = document.getElementById('editor');
        field.value = ${JSON.stringify(source)};
        // What the app hears: each field event's InputEvent fields. \`prevent(e)\` cancels a beforeinput.
        window.events = []; window.logs = []; window.prevent = () => false;
        const record = (type, value, start, end, direction, inputType, data, cancelable, composing) => events.push({ type, value, start, end, direction, inputType, data, cancelable, composing });
        if (${runtime}) {
          const { mde, on, onKey, onBeforeInput, onSelectionChange, onValue, Hosts } = await import('./rt.js');
          const commands = await import('./commands.js');
          window.command = (name, args) => commands[name](s => logs.push(s))(...args);
          on(field, 'beforeinput', r => { record('beforeinput', r[0], ...r.slice(2)); if (prevent(r)) Hosts.preventDefault(); }, onBeforeInput);
          on(field, 'selectionchange', r => record('selectionchange', r[0], ...r.slice(2)), onSelectionChange);
          on(field, 'input', (v, r) => record('input', v, ...r.slice(2)), onValue);
          window.heard = [];
          on(document.body, 'keycapture', key => {
            heard.push('outer:' + key);
            if (key === 's') Hosts.stopPropagation();
            if (key === 'p') Hosts.preventDefault();
          }, onKey);
          on(document.body, 'key', key => heard.push('bubble:' + key), onKey);
          on(field, 'key', key => heard.push('key:' + key), onKey);
          on(field, 'keycapture', key => {
            heard.push('capture:' + key);
            if (key === 'x' || key.toLowerCase() === 'b') Hosts.preventDefault();
            if (key === 't') Hosts.stopPropagation();
          }, onKey);
          window.original = field; // the runtime's element, whose attributes it forwards to the editor
          mde(field);
        } else {
          await import('./markup-editor.js');
          const { runFocusCommands } = await import('./navigation.js');
          window.command = (name, args) => runFocusCommands([{ name, args }], { root: document.body, ready: true, inertAncestor: () => false, log: s => logs.push(s) });
          const install = await exact.installMarkupEditor;
          const host = { live: el => el.isConnected, select() {}, replace(el) {
            field = el;
            const heard = e => record(e.type, el.value, el.selectionStart, el.selectionEnd, el.selectionDirection, e.inputType ?? '', e.data ?? null, e.cancelable ?? false, e.isComposing ?? false);
            field.addEventListener('beforeinput', e => { heard(e); if (prevent(e)) e.preventDefault(); });
            field.addEventListener('selectionchange', heard);
            field.addEventListener('input', e => { heard(e); document.getElementById('echo').textContent = field.value; field.value = field.value; });
          } };
          window.restoreMarkup = () => { field.setAttribute('markup', 'markdown'); field = install(field, host); };
          field = install(field, host);
          window.field = () => field;
          window.ready = true;
        }
      </script>`, { headers: { 'content-type': 'text/html' } });
  } });
  const context = await (await browser(engine)).newContext();
  try {
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(`http://127.0.0.1:${server.port}/`);
    await page.waitForFunction(runtime ? () => document.getElementById('editor')?.exactMarkup : () => window.ready);
    await drive(page, page.locator('#editor'));
    expect(errors).toEqual([]);
  } finally { await context.close(); server.stop(true); }
}

// One browser per engine for this file; each editor above gets a context of
// its own (its page, its undo, its storage), closed when it is done.
const launched = new Map();
function browser(engine) {
  if (!launched.has(engine)) launched.set(engine, suiteBrowser(engine));
  return launched.get(engine);
}
afterAll(async () => { for (const b of launched.values()) await (await b.catch(() => null))?.close(); }, 60000);

async function dragSelection(page, field, from, to, destination, copy = false, changed = true) {
  await field.focus();
  const points = await field.evaluate((el, [from, to, destination]) => {
    el.setSelectionRange(from, to);
    const start = el.exactMarkup.rangeFrame(from, to);
    const end = typeof destination === 'number' ? el.exactMarkup.rangeFrame(destination, destination) : null;
    return { from: { x: start.x + start.width / 2, y: start.y + start.height / 2 },
      to: end ? { x: end.x, y: end.y + end.height / 2 } : destination };
  }, [from, to, destination]);
  await settle(page);
  await field.evaluate((el, changed) => {
    // A changed drop can detach the source, so finish at its input.
    // A cancelled drop keeps the source and finishes without an input.
    window.dragDone = new Promise(ok => el.addEventListener(changed ? 'input' : 'dragend', ok, { once: true }));
  }, changed);
  const cdp = page.context().browser().browserType().name() === 'chromium' ? await page.context().newCDPSession(page) : null;
  if (copy) await page.keyboard.down('Alt');
  try {
    await page.mouse.move(points.from.x, points.from.y);
    if (cdp) {
      await cdp.send('Input.setInterceptDrags', { enabled: true });
      const dragged = new Promise(ok => cdp.once('Input.dragIntercepted', ok));
      const timestamp = Date.now() / 1000, modifiers = copy ? 1 : 0;
      // Chromium's macOS text-drag threshold is time since the press.
      // Encode the held press in native timestamps, without sleeping.
      await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', ...points.from,
        button: 'left', buttons: 1, clickCount: 1, modifiers, timestamp: timestamp - 0.25 });
      await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: points.from.x + 8, y: points.from.y + 8,
        button: 'left', buttons: 1, modifiers, timestamp });
      const { data } = await dragged;
      for (const type of ['dragEnter', 'dragOver', 'drop']) {
        await cdp.send('Input.dispatchDragEvent', { type, ...points.to, data, modifiers });
      }
    } else {
      await page.mouse.down();
      await page.mouse.move(points.from.x + 8, points.from.y + 8);
      await page.mouse.move(points.to.x, points.to.y, { steps: 12 });
      await page.mouse.up();
    }
    await page.evaluate(() => window.dragDone.then(() => {}));
  } finally {
    if (copy) await page.keyboard.up('Alt');
    if (cdp) { await cdp.send('Input.setInterceptDrags', { enabled: false }); await cdp.detach(); }
  }
}

// WebKit's macOS text-drag delay cannot be advanced by its pinned protocol:
// Input.dispatchMouseEvent has no timestamp. Exercise its source-range drop
// inputs explicitly; Chromium and Firefox below exercise native gestures.
async function dropSelection(page, field, from, to, destination) {
  await field.focus();
  const pending = await field.evaluate((el, [from, to, destination]) => {
    el.setSelectionRange(from, to);
    const range = getSelection().getRangeAt(0), source = range.startContainer;
    const target = new StaticRange({ startContainer: source, startOffset: range.startOffset,
      endContainer: range.endContainer, endOffset: range.endOffset });
    const end = el.exactMarkup.rangeFrame(destination, destination), text = el.value.slice(from, to);
    el.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true,
      clientX: end.x, clientY: end.y + end.height / 2 }));
    el.dispatchEvent(new InputEvent('beforeinput', { inputType: 'deleteByDrag',
      bubbles: true, cancelable: true, targetRanges: [target] }));
    const pending = [el.value, source.isConnected];
    // WebKit retains the source selection as the insertion range after a
    // cancelled native deletion. The drop coordinates supply the caret.
    el.dispatchEvent(new InputEvent('beforeinput', { inputType: 'insertFromDrop', data: text,
      bubbles: true, cancelable: true, targetRanges: [target] }));
    return pending;
  }, [from, to, destination]);
  expect(pending[1]).toBe(true);
  return pending[0];
}

for (const engine of engines) engineTest[engine](`${engine}: ${engine === 'webkit' ? 'Markdown drop inputs' : 'native Markdown drags'} move text in one undo step`, async () => {
  for (const [source, word, at, expected, wrap] of [
    ['alpha beta gamma delta', 'beta', 22, 'alpha  gamma deltabeta'],
    ['alpha beta gamma delta', 'beta', 0, 'betaalpha  gamma delta'],
    ['alpha **beta** gamma delta', 'beta', 26, 'alpha  gamma deltabeta'],
    ['- alpha beta gamma delta', 'beta', 24, '- alpha  gamma deltabeta'],
    ['alpha beta gamma delta', 'beta', 22, 'alpha  gamma deltabeta', true],
    ['alpha beta gamma\ndelta', 'beta', 22, 'alpha  gamma\ndeltabeta'],
    ['alpha beta😀 gamma delta', 'beta😀', 24, 'alpha  gamma deltabeta😀'],
  ]) {
    await editor(engine, source, async (page, field) => {
      if (wrap) await field.evaluate(el => { el.style.width = '260px'; el.style.font = '20px monospace'; });
      const from = source.indexOf(word);
      if (engine === 'webkit') expect(await dropSelection(page, field, from, from + word.length, at)).toBe(source);
      else await dragSelection(page, field, from, from + word.length, at);
      expect(await field.evaluate(el => el.value)).toBe(expected);
      const events = await heard(page);
      expect(events.filter(e => e.type === 'beforeinput').map(e => e.inputType)).toEqual(['deleteByDrag', 'insertFromDrop']);
      expect(events.filter(e => e.type === 'input').map(e => e.value)).toEqual([expected]);
      await page.keyboard.press('ControlOrMeta+Z');
      expect(await field.evaluate(el => el.value)).toBe(source);
      await page.keyboard.press('ControlOrMeta+Shift+Z');
      expect(await field.evaluate(el => el.value)).toBe(expected);
    });
  }
}, 60000);

engineTest.chromium('chromium: a Markdown drag can copy, leave the editor or be cancelled', async () => {
  const source = 'alpha beta gamma delta';
  for (const operation of ['copy', 'outside', 'cancel-drop', 'cancel-delete', 'cancel-insert']) {
    await editor('chromium', source, async (page, field) => {
      let destination = source.length;
      if (operation === 'outside') {
        destination = await field.evaluate(el => {
          const other = document.createElement('textarea');
          other.id = 'other'; el.after(other);
          const box = other.getBoundingClientRect();
          return { x: box.x + 5, y: box.y + 5 };
        });
      } else if (operation.startsWith('cancel-')) {
        await field.evaluate((el, operation) => {
          if (operation === 'cancel-drop') el.addEventListener('drop', e => e.preventDefault());
          else window.prevent = e => e.inputType === (operation === 'cancel-delete' ? 'deleteByDrag' : 'insertFromDrop');
        }, operation);
      }
      await dragSelection(page, field, 6, 10, destination, operation === 'copy', operation !== 'cancel-drop');
      const expected = ['copy', 'cancel-delete'].includes(operation) ? source + 'beta'
        : operation === 'cancel-drop' ? source : 'alpha  gamma delta';
      expect(await field.evaluate(el => el.value)).toBe(expected);
      if (operation === 'outside') expect(await page.locator('#other').inputValue()).toBe('beta');
      if (operation === 'copy') expect((await heard(page)).filter(e => e.type === 'beforeinput').map(e => e.inputType)).toEqual(['insertFromDrop']);
      if (operation !== 'cancel-drop') {
        await field.focus();
        await page.keyboard.press('ControlOrMeta+Z');
        expect(await field.evaluate(el => el.value)).toBe(source);
      }
    });
  }
}, 60000);

for (const engine of engines) {
  engineTest[engine](`${engine}: a drag deletion settles without a drop and retires with its source`, async () => {
    await editor(engine, 'alpha beta gamma delta', async (page, field) => {
      await field.focus();
      expect(await field.evaluate(async el => {
        el.setSelectionRange(6, 10);
        const done = new Promise(ok => el.addEventListener('input', ok, { once: true }));
        el.dispatchEvent(new InputEvent('beforeinput', { inputType: 'deleteByDrag', bubbles: true, cancelable: true }));
        const pending = el.value;
        await done;
        return [pending, el.value];
      })).toEqual(['alpha beta gamma delta', 'alpha  gamma delta']);
      await page.keyboard.press('ControlOrMeta+Z');
      expect(await field.evaluate(el => el.value)).toBe('alpha beta gamma delta');
      for (const retire of ['replace', 'destroy']) {
        expect(await field.evaluate(async (el, retire) => {
          el.setSelectionRange(6, 10);
          el.dispatchEvent(new InputEvent('beforeinput', { inputType: 'deleteByDrag', bubbles: true, cancelable: true }));
          if (retire === 'replace') el.value = 'a new source';
          else el.exactMarkup.destroy();
          // Run after the deletion's queued task, including when it was cleared.
          await new Promise(ok => setTimeout(ok, 0));
          return el.value;
        }, retire)).toBe('a new source');
      }
    });
  }, 60000);

  engineTest[engine](`${engine}: formatting whole Markdown lines keeps their block markers`, async () => {
    const source = '# Title\n\n- one\n- [ ] task\n> quote';
    await editor(engine, source, async (page, field) => {
      await field.focus();
      await field.evaluate(el => getSelection().selectAllChildren(el));
      await page.keyboard.press('ControlOrMeta+B');
      expect(await field.evaluate(el => el.value)).toBe('# **Title**\n\n- **one**\n- [ ] **task**\n> **quote**');
      await page.keyboard.press('ControlOrMeta+Z');
      expect(await field.evaluate(el => el.value)).toBe(source);
    });
  }, 60000);

  engineTest[engine](`${engine}: emptying inline Markdown leaves no rule or code fence`, async () => {
    await editor(engine, '', async (page, field) => {
      await field.focus();
      await page.keyboard.press('ControlOrMeta+B');
      await page.keyboard.type('hi');
      expect(await field.evaluate(el => el.value)).toBe('**hi**');
      await page.keyboard.press('Backspace');
      await page.keyboard.press('Backspace');
      expect(await field.evaluate(el => [el.value, el.selectionStart, el.selectionEnd])).toEqual(['', 0, 0]);
      expect(await field.locator('.md-rule, .md-code').count()).toBe(0);
      await page.keyboard.type('x');
      expect(await field.evaluate(el => el.value)).toBe('x');
    });
    for (const [source, expected] of [
      ['intro\n\n~~x~~\n\nnext paragraph', 'intro\n\n\n\nnext paragraph'],
      ['~~**_`x`_**~~\n\n**later**', '\n\n**later**'],
      ['**~~_`x`_~~**\n\n**later**', '\n\n**later**'],
      ['~~x~~ **later**', ' **later**'],
      ['~~x~~ [later](https://example.com)', ' [later](https://example.com)'],
      ['   ~~x~~ ~~later~~', '    ~~later~~'],
      ['``x`` **later**', ' **later**'],
      ['~~**_`x`_**~~ [later](https://example.com)', ' [later](https://example.com)'],
      ['~~x~~**later**', '**later**'],
      ['**x**~~later~~', '~~later~~'],
      ['> x\n> keep', '\n> keep'],
      ['> **x**\n> keep', '\n> keep'],
      ['`x`\n``\nnext', '\n``\nnext'],
      ['**`x`**\n``\nnext', '\n``\nnext'],
    ]) {
      await editor(engine, source, async (page, field) => {
        await field.focus();
        await field.evaluate(el => {
          const at = el.value.indexOf('x') + 1;
          el.setSelectionRange(at, at);
        });
        await page.keyboard.press('Backspace');
        expect(await field.evaluate(el => el.value)).toBe(expected);
        expect(await field.locator('.md-rule, .md-code').count()).toBe(0);
        await page.keyboard.press('ControlOrMeta+Z');
        expect(await field.evaluate(el => el.value)).toBe(source);
      });
    }
  }, 60000);

  engineTest[engine](`${engine}: Return in formatted Markdown keeps links and a Unicode caret`, async () => {
    for (const [source, start, end, expected, position] of [
      ['- [docs](https://example.com) here', 3, 3, '- \n- [docs](https://example.com) here', 5],
      ['- **Note:** details', 4, 4, '- \n- **Note:** details', 5],
      ['- **bold**', 6, 6, '- **bo**\n- **ld**', 13],
      ['- **a😀b**', 5, 5, '- **a**\n- **😀b**', 12],
      ['# [link](https://example.com) title', 5, 5, '# [li](https://example.com)\n[nk](https://example.com) title', 29],
      ['- [docs](https://example.com)', 5, 7, '- [do](https://example.com)\n- ', 30],
      ['- <https://example.com>', 14, 14, '- [https\\://exa](https://example.com)\n- [mple.com](https://example.com)', 41],
    ]) {
      await editor(engine, source, async (page, field) => {
        await field.focus();
        await field.evaluate((el, range) => el.setSelectionRange(...range), [start, end]);
        await page.keyboard.press('Enter');
        expect(await field.evaluate(el => [el.value, el.selectionStart, el.selectionEnd])).toEqual([expected, position, position]);
        await page.keyboard.press('ControlOrMeta+Z');
        expect(await field.evaluate(el => el.value)).toBe(source);
        await page.keyboard.press('ControlOrMeta+Shift+Z');
        expect(await field.evaluate(el => el.value)).toBe(expected);
        await page.keyboard.type('X');
        expect(await field.evaluate(el => el.value)).toBe(expected.slice(0, position) + 'X' + expected.slice(position));
        await page.keyboard.press('ControlOrMeta+Z');
        expect(await field.evaluate(el => el.value)).toBe(expected);
      });
    }
  }, 60000);

  engineTest[engine](`${engine}: Return and Backspace preserve nested Markdown and link targets`, async () => {
    for (const [source, position, split, backspaced] of [
      ['- **bold**', 6, '- **bo**\n- **ld**', '- **bo**\n**ld**'],
      ['- [docs](u)', 5, '- [do](u)\n- [cs](u)', '- [do](u)\n[cs](u)'],
      ['- **`a b`**', 6, '- **`a`**\n- **` b`**', '- **`a`**\n**` b`**'],
      ['- **`a b`**', 7, '- **`a `**\n- **`b`**', '- **`a `**\n**`b`**'],
      ['- **a😀b**', 5, '- **a**\n- **😀b**', '- **a**\n**😀b**'],
      ['> - [x] **bold**', 12, '> - [x] **bo**\n> - [ ] **ld**', '> - [x] **bo**\n**ld**'],
    ]) {
      await editor(engine, source, async (page, field) => {
        await field.focus();
        await field.evaluate((el, p) => el.setSelectionRange(p, p), position);
        await page.keyboard.press('Enter');
        expect(await field.evaluate(el => el.value)).toBe(split);
        await page.keyboard.press('Backspace');
        expect(await field.evaluate(el => el.value)).toBe(backspaced);
        await page.keyboard.press('ControlOrMeta+Z');
        expect(await field.evaluate(el => el.value)).toBe(split);
        await page.keyboard.press('ControlOrMeta+Shift+Z');
        expect(await field.evaluate(el => el.value)).toBe(backspaced);
      });
    }
  }, 60000);

  engineTest[engine](`${engine}: JS runtime forwards Markdown key cancellation in both directions`, async () => {
    await editor(engine, 'hello', async (page, field) => {
      await field.focus();
      await field.evaluate(el => getSelection().selectAllChildren(el));
      await page.keyboard.press('x');
      expect(await field.evaluate(el => el.value)).toBe('hello');
      expect(await page.evaluate(() => heard.splice(0))).toEqual(['outer:x', 'capture:x', 'key:x', 'bubble:x']);
      await page.keyboard.press('p');
      expect(await field.evaluate(el => el.value)).toBe('hello');
      expect(await page.evaluate(() => heard.splice(0))).toEqual(['outer:p', 'capture:p', 'key:p', 'bubble:p']);
      await page.keyboard.press('s');
      expect(await field.evaluate(el => el.value)).toBe('s');
      expect(await page.evaluate(() => heard.splice(0))).toEqual(['outer:s']);
      await page.keyboard.press('t');
      expect(await field.evaluate(el => el.value)).toBe('st');
      expect(await page.evaluate(() => heard.splice(0))).toEqual(['outer:t', 'capture:t']);
      await field.evaluate(el => getSelection().selectAllChildren(el));
      await page.keyboard.press('ControlOrMeta+B');
      expect(await field.evaluate(el => el.value)).toBe('st');
    }, true);
  }, 60000);

  engineTest[engine](`${engine}: a cancelled capture shortcut leaves selected Markdown unchanged`, async () => {
    await editor(engine, 'hello', async (page, field) => {
      await field.focus();
      await field.evaluate(el => {
        getSelection().selectAllChildren(el);
        document.addEventListener('keydown', e => {
          if (e.isTrusted && e.key.toLowerCase() === 'b' && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            e.exactStopped = true;
            e.$stopped = true;
          }
        }, true);
      });
      await page.keyboard.press('ControlOrMeta+B');
      expect(await field.evaluate(el => el.value)).toBe('hello');
      for (const flag of ['exactStopped', '$stopped']) {
        await field.evaluate((el, flag) => {
          const event = new KeyboardEvent('keydown', { key: 'b', ctrlKey: true, bubbles: true, cancelable: true });
          event[flag] = true;
          el.dispatchEvent(event);
        }, flag);
        expect(await field.evaluate(el => el.value)).toBe('hello');
      }
    });
  }, 60000);

  engineTest[engine](`${engine}: browser Select All includes hidden syntax`, async () => {
    for (const source of ['# Heading', '```js\ncode\n```', '# Heading\n\n[link](https://example.com)\n\n```js\ncode\n```']) {
      await editor(engine, source, async (page, field) => {
        await field.focus();
        await field.evaluate(() => document.execCommand('selectAll'));
        await page.keyboard.insertText('replacement');
        expect(await field.evaluate(el => el.value)).toBe('replacement');
        await page.keyboard.press('ControlOrMeta+Z');
        expect(await field.evaluate(el => el.value)).toBe(source);
      });
    }
    await editor(engine, '# Heading\n\nBody', async (page, field) => {
      await field.focus();
      await field.evaluate(el => {
        const text = el.querySelector('.md-h1').lastChild;
        getSelection().setBaseAndExtent(text, 0, text, text.textContent.length);
      });
      await page.keyboard.insertText('Changed');
      expect(await field.evaluate(el => el.value)).toBe('# Changed\n\nBody');
    });
  }, 60000);

  engineTest[engine](`${engine}: a thematic break's line is the field's ink, dimmed as shown syntax is, light and dark`, async () => {
    await editor(engine, 'Above\n\n---', async (page, field) => {
      const look = scheme => field.evaluate((el, scheme) => {
        document.documentElement.style.colorScheme = scheme;
        const line = getComputedStyle(el.querySelector('.md-rule'), '::after');
        return [line.borderTopColor, getComputedStyle(el).color, line.opacity];
      }, scheme);
      const light = await look('light'), dark = await look('dark');
      for (const [line, ink, opacity] of [light, dark]) expect([line, opacity]).toEqual([ink, '0.45']);
      expect(light[1]).not.toBe(dark[1]);
    });
  }, 60000);

  engineTest[engine](`${engine}: multi-line replacement reports only the new source and undo restores every line`, async () => {
    for (const source of ['one\n\ntwo', '# Heading\n\n[link](https://example.com)\n\n```js\ncode\n```', '- [x] done\n\n> quoted\n\n- two']) {
      await editor(engine, source, async (page, field) => {
        // Untouched hidden constructs also survive the source-mode round trip.
        await field.evaluate(el => { el.removeAttribute('markup'); el.exactMarkup.sync(); });
        expect(await field.inputValue()).toBe(source);
        await page.evaluate(() => restoreMarkup());
        expect(await field.evaluate(el => el.value)).toBe(source);
        await field.click();
        await page.keyboard.press('ControlOrMeta+A');
        await page.keyboard.insertText('replacement');
        expect(await field.evaluate(el => el.value)).toBe('replacement');
        expect(await page.locator('#echo').textContent()).toBe('replacement');
        await page.keyboard.press('ControlOrMeta+Z');
        expect(await field.evaluate(el => el.value)).toBe(source);
        expect(await field.evaluate(el => [...el.children].map(line => line.textContent).join('\n'))).toBe(source);
        await page.keyboard.press('ControlOrMeta+Shift+Z');
        expect(await field.evaluate(el => el.value)).toBe('replacement');
        expect(await field.textContent()).toBe('replacement');
        // Source mode receives the same controlled value after replacement.
        await field.evaluate(el => { el.removeAttribute('markup'); el.exactMarkup.sync(); });
        expect(await field.inputValue()).toBe('replacement');
        await page.evaluate(() => restoreMarkup());
        expect(await field.evaluate(el => el.value)).toBe('replacement');
      });
    }
  }, 60000);

  engineTest[engine](`${engine}: native structural edits and composition reconcile the live tree`, async () => {
    await editor(engine, 'one\n\ntwo', async (page, field) => {
      await field.click();
      await field.evaluate(el => {
        window.nativeLine = el.firstChild;
        getSelection().setBaseAndExtent(el.firstChild.firstChild, 3, el.firstChild.firstChild, 3);
      });
      await page.keyboard.type('!');
      expect(await field.evaluate(el => el.value)).toBe('one!\n\ntwo');
      expect(await field.evaluate(el => el.firstChild === window.nativeLine)).toBe(true);
      await page.keyboard.press('ControlOrMeta+Z');
      expect(await field.evaluate(el => el.value)).toBe('one\n\ntwo');
      // execCommand is a real browser edit that omits beforeinput. It forces
      // the native reconciliation path even when shared rules handle typing.
      await field.evaluate(el => { getSelection().selectAllChildren(el); document.execCommand('insertText', false, 'native'); });
      expect(await field.evaluate(el => el.value)).toBe('native');
      await page.keyboard.press('ControlOrMeta+Z');
      expect(await field.evaluate(el => [...el.children].map(line => line.textContent).join('\n'))).toBe('one\n\ntwo');
      await field.evaluate(el => {
        el.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
        getSelection().selectAllChildren(el);
        document.execCommand('insertText', false, 'composed\nnext');
      });
      // While composing, the value is the text the input method left, as a plain field's.
      expect(await field.evaluate(el => el.value)).toBe('composed\nnext');
      await field.evaluate(el => el.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true })));
      expect(await field.evaluate(el => el.value)).toBe('composed\nnext');
      expect(await page.locator('#echo').textContent()).toBe('composed\nnext');
      await page.keyboard.press('ControlOrMeta+Z');
      expect(await field.evaluate(el => [...el.children].map(line => line.textContent).join('\n'))).toBe('one\n\ntwo');
      await page.keyboard.press('ControlOrMeta+Shift+Z');
      expect(await field.evaluate(el => el.value)).toBe('composed\nnext');
    });
  }, 60000);

  engineTest[engine](`${engine}: composition replaces selected hidden fences and preserves cancelled compositions`, async () => {
    const source = '# Heading\n\n[link](https://example.com)\n\n```js\ncode\n```';
    await editor(engine, source, async (page, field) => {
      await field.click();
      await page.keyboard.press('ControlOrMeta+A');
      await field.evaluate(el => {
        el.dispatchEvent(new CompositionEvent('compositionstart'));
        el.dispatchEvent(new CompositionEvent('compositionend'));
      });
      expect(await field.evaluate(el => el.value)).toBe(source);
      await page.keyboard.press('ControlOrMeta+A');
      await field.evaluate(el => {
        el.dispatchEvent(new CompositionEvent('compositionstart'));
        document.execCommand('insertText', false, 'composed\nnext');
        el.dispatchEvent(new CompositionEvent('compositionend'));
      });
      expect(await field.evaluate(el => el.value)).toBe('composed\nnext');
      expect(await page.locator('#echo').textContent()).toBe('composed\nnext');
      await page.keyboard.press('ControlOrMeta+Z');
      expect(await field.evaluate(el => [...el.children].map(line => line.textContent).join('\n'))).toBe(source);
      await page.keyboard.press('ControlOrMeta+Shift+Z');
      expect(await field.evaluate(el => el.value)).toBe('composed\nnext');
    });
  }, 60000);
}

// A frame and a task: the document's own selectionchange, and what it queues, have run.
const settle = page => page.evaluate(() => new Promise(ok => requestAnimationFrame(() => setTimeout(ok))));
const heard = page => page.evaluate(() => events.splice(0));
const only = (list, type) => list.filter(e => e.type === type).map(({ value, start, end, direction, inputType, data, cancelable, composing }) => [value, start, end, direction, inputType, data, cancelable, composing]);

for (const engine of engines) for (const runtime of [false, true]) {
  // @ref LLP 1045 D11 — the app's base direction for each line, as HTML's `dir`.
  engineTest[engine](`${engine}${runtime ? ' (JS runtime)' : ''}: each line takes its paragraphDirs word, in place`, async () => {
    await editor(engine, 'One\nاثنان ثلاثة\n- [ ] أربعة\n', async (page, field) => {
      // The app's words: the runtime writes the textarea it replaced and forwards them; the glue syncs the editor.
      const say = async words => {
        await page.evaluate(([words, runtime]) => {
          (runtime ? window.original : field()).setAttribute('data-paragraph-dirs', words);
          if (!runtime) field().exactMarkup.sync();
        }, [words, runtime]);
        await settle(page);
      };
      const dirs = () => field.evaluate(el => [...el.children].map(line => line.getAttribute('dir')));
      await say('ltr rtl');
      // A line past the last word takes the last: the list item and the empty last line.
      expect(await dirs()).toEqual(['ltr', 'rtl', 'rtl', 'rtl']);
      // A right-to-left list item's marker and gutter are on its right, where a press toggles its box.
      const gutter = await field.evaluate(el => {
        const line = el.children[2], box = line.getBoundingClientRect(), css = getComputedStyle(line);
        return { x: box.right - 10, y: box.top + box.height / 2, left: css.paddingLeft, right: css.paddingRight };
      });
      expect([gutter.left, gutter.right]).toEqual(['0px', '28px']);
      await page.mouse.click(gutter.x, gutter.y); await settle(page);
      expect(await field.evaluate(el => el.value)).toBe('One\nاثنان ثلاثة\n- [x] أربعة\n');
      // The empty last line's caret is at its start, the right.
      expect(await field.evaluate(el => {
        const caret = el.exactMarkup.rangeFrame(el.value.length, el.value.length), box = el.getBoundingClientRect();
        return caret.x > box.x + box.width / 2;
      })).toBe(true);
      // Typing keeps the line's node and its word: a direction never redraws a line.
      await field.evaluate(el => { window.kept = el.children[1]; el.focus(); el.setSelectionRange(15, 15); });
      await page.keyboard.type('y'); await settle(page);
      expect(await field.evaluate(el => [el.value.split('\n')[1], el.children[1] === window.kept, window.kept.getAttribute('dir')])).toEqual(['اثنان ثلاثةy', true, 'rtl']);
      // Another word leaves its line, and those past it, to the field.
      await say('rtl sideways');
      expect(await dirs()).toEqual(['rtl', null, null, null]);
      await say('');
      expect(await dirs()).toEqual([null, null, null, null]);
    }, runtime);
  }, 60000);

  engineTest[engine](`${engine}${runtime ? ' (JS runtime)' : ''}: a word stays with its line through an edit that adds or removes one`, async () => {
    await editor(engine, 'A\nب\nC', async (page, field) => {
      const dirs = () => field.evaluate(el => [...el.children].map(line => line.getAttribute('dir')));
      // Words the page wrote before the editor mounted.
      expect(await dirs()).toEqual(['ltr', 'rtl', 'ltr']);
      // Return at the end of the first line: the new line takes the word above it, and ب keeps its own.
      await field.evaluate(el => { el.focus(); el.setSelectionRange(1, 1); });
      await page.keyboard.press('Enter'); await settle(page);
      expect(await field.evaluate(el => el.value)).toBe('A\n\nب\nC');
      expect(await dirs()).toEqual(['ltr', 'ltr', 'rtl', 'ltr']);
      // Delete at the empty line takes it back out (forward: WebKit's Backspace navigates in the runtime harness).
      await page.keyboard.press('Delete'); await settle(page);
      expect(await field.evaluate(el => el.value)).toBe('A\nب\nC');
      expect(await dirs()).toEqual(['ltr', 'rtl', 'ltr']);
      // The app's next words go by line number again.
      await page.evaluate(runtime => {
        (runtime ? window.original : field()).setAttribute('data-paragraph-dirs', 'rtl');
        if (!runtime) field().exactMarkup.sync();
      }, runtime);
      await settle(page);
      expect(await dirs()).toEqual(['rtl', 'rtl', 'rtl']);
    }, runtime, 'data-paragraph-dirs="ltr rtl ltr"');
  }, 60000);

  engineTest[engine](`${engine}${runtime ? ' (JS runtime)' : ''}: undo and redo give back the words of the lines they restore`, async () => {
    await editor(engine, 'A\nب\nC', async (page, field) => {
      const dirs = () => field.evaluate(el => [...el.children].map(line => line.getAttribute('dir')));
      await field.evaluate(el => { el.focus(); el.setSelectionRange(1, el.value.length); });
      await page.keyboard.press('Delete'); await settle(page);
      expect([await field.evaluate(el => el.value), await dirs()]).toEqual(['A', ['ltr']]);
      await page.keyboard.press('ControlOrMeta+Z'); await settle(page);
      expect([await field.evaluate(el => el.value), await dirs()]).toEqual(['A\nب\nC', ['ltr', 'rtl', 'ltr']]);
      await page.keyboard.press('ControlOrMeta+Shift+Z'); await settle(page);
      expect([await field.evaluate(el => el.value), await dirs()]).toEqual(['A', ['ltr']]);
      await page.keyboard.press('ControlOrMeta+Z'); await settle(page);
      expect(await dirs()).toEqual(['ltr', 'rtl', 'ltr']);
    }, runtime, 'data-paragraph-dirs="ltr rtl ltr"');
  }, 60000);

  engineTest[engine](`${engine}${runtime ? ' (JS runtime)' : ''}: undo steps keep their words, which typing and the app's next words do not bring back`, async () => {
    const dirs = field => field.evaluate(el => [...el.children].map(line => line.getAttribute('dir')));
    const value = field => field.evaluate(el => el.value);
    // 64 typed characters are one undo step: the second undo still gives back the first words.
    // (No x or b: the runtime harness's key handlers cancel them.)
    await editor(engine, 'A\nب\nC', async (page, field) => {
      await field.evaluate(el => { el.focus(); el.setSelectionRange(1, el.value.length); });
      await page.keyboard.press('Delete'); await settle(page);
      await page.keyboard.type('y'.repeat(64)); await settle(page);
      expect(await value(field)).toBe(`A${'y'.repeat(64)}`);
      await page.keyboard.press('ControlOrMeta+Z'); await settle(page);
      expect(await value(field)).toBe('A');
      await page.keyboard.press('ControlOrMeta+Z'); await settle(page);
      expect([await value(field), await dirs(field)]).toEqual(['A\nب\nC', ['ltr', 'rtl', 'ltr']]);
    }, runtime, 'data-paragraph-dirs="ltr rtl ltr"');
    // A removed line typed back in takes the word above it, not the one it had.
    await editor(engine, 'A\nQ', async (page, field) => {
      await field.evaluate(el => { el.focus(); el.setSelectionRange(1, 3); });
      await page.keyboard.press('Delete'); await page.keyboard.press('Enter'); await page.keyboard.type('Q'); await settle(page);
      expect([await value(field), await dirs(field)]).toEqual(['A\nQ', ['ltr', 'ltr']]);
    }, runtime, 'data-paragraph-dirs="ltr rtl"');
    // The app's next words forget those undo would have given back, and the
    // step undo returns to keeps the words drawn then, for the next undo.
    await editor(engine, 'A\nB\nC', async (page, field) => {
      await field.evaluate(el => { el.focus(); el.setSelectionRange(3, 3); });
      await page.keyboard.type('y'); await settle(page);
      await page.evaluate(runtime => {
        (runtime ? window.original : field()).setAttribute('data-paragraph-dirs', 'ltr rtl rtl');
        if (!runtime) field().exactMarkup.sync();
      }, runtime);
      await settle(page);
      await page.keyboard.press('ControlOrMeta+Z'); await settle(page);
      expect([await value(field), await dirs(field)]).toEqual(['A\nB\nC', ['ltr', 'rtl', 'rtl']]);
      await field.evaluate(el => el.setSelectionRange(2, 4));
      await page.keyboard.press('Delete'); await settle(page);
      expect(await value(field)).toBe('A\nC');
      await page.keyboard.press('ControlOrMeta+Z'); await settle(page);
      expect([await value(field), await dirs(field)]).toEqual(['A\nB\nC', ['ltr', 'rtl', 'rtl']]);
    }, runtime, 'data-paragraph-dirs="ltr rtl ltr"');
    // The app's setRangeText is an undo step of its own, which keeps its words.
    await editor(engine, 'A\nب\nC', async (page, field) => {
      await field.evaluate(el => { el.focus(); el.setRangeText('', 1, 3); });
      await settle(page);
      expect([await value(field), await dirs(field)]).toEqual(['A\nC', ['ltr', 'ltr']]);
      await page.keyboard.press('ControlOrMeta+Z'); await settle(page);
      expect([await value(field), await dirs(field)]).toEqual(['A\nب\nC', ['ltr', 'rtl', 'ltr']]);
    }, runtime, 'data-paragraph-dirs="ltr rtl ltr"');
  }, 60000);

  engineTest[engine](`${engine}${runtime ? ' (JS runtime)' : ''}: quote bars, list markers and the placeholder sit at each line's start`, async () => {
    await editor(engine, '> اقتباس\n- عنصر', async (page, field) => {
      // Each line's start border and padding, physical: [left border, right border, left padding, right padding].
      const sides = () => field.evaluate(el => [...el.children].map(line => {
        const css = getComputedStyle(line);
        return [css.borderLeftWidth, css.borderRightWidth, css.paddingLeft, css.paddingRight];
      }));
      // The list marker's own box, its resolved left and right insets: at the start side, 0.
      const marker = () => field.evaluate(el => { const css = getComputedStyle(el.children[1], '::before'); return [css.left === '0px', css.right === '0px']; });
      expect(await sides()).toEqual([['18px', '0px', '0px', '0px'], ['0px', '0px', '28px', '0px']]);
      expect(await marker()).toEqual([true, false]);
      // A field whose own direction is right to left, with no words: CSS direction alone.
      await field.evaluate(el => { el.style.direction = 'rtl'; });
      expect(await sides()).toEqual([['0px', '18px', '0px', '0px'], ['0px', '0px', '0px', '28px']]);
      expect(await marker()).toEqual([false, true]);
      await field.evaluate(el => { el.style.direction = ''; });
      await page.evaluate(runtime => {
        (runtime ? window.original : field()).setAttribute('data-paragraph-dirs', 'rtl');
        if (!runtime) field().exactMarkup.sync();
      }, runtime);
      await settle(page);
      expect(await sides()).toEqual([['0px', '18px', '0px', '0px'], ['0px', '0px', '0px', '28px']]);
      expect(await marker()).toEqual([false, true]);
      // Emptied, the placeholder is drawn on the first line, in that line's direction.
      await field.evaluate(el => { el.value = ''; });
      await settle(page);
      expect(await field.evaluate(el => {
        const line = el.children[0];
        return [el.value, line.getAttribute('dir'), getComputedStyle(line, '::before').content, getComputedStyle(line, '::before').direction];
      })).toEqual(['', 'rtl', '"Ask"', 'rtl']);
    }, runtime, 'placeholder="Ask"');
  }, 60000);

  engineTest[engine](`${engine}${runtime ? ' (JS runtime)' : ''}: source selection, range edits, their undo and the app's beforeinput`, async () => {
    await editor(engine, 'hello', async (page, field) => {
      const command = (name, ...args) => page.evaluate(([name, args]) => command(name, args), [name, args]);
      const state = () => field.evaluate(el => [el.value, el.selectionStart, el.selectionEnd, el.selectionDirection]);
      await field.focus(); await settle(page); await heard(page);
      // A script's selection: source offsets and its direction, then one selectionchange.
      await command('setSelectionRange', 'editor', 1, 4, 'backward'); await settle(page);
      expect(await state()).toEqual(['hello', 1, 4, 'backward']);
      expect(only(await heard(page), 'selectionchange')).toEqual([['hello', 1, 4, 'backward', '', null, false, false]]);
      // Two range edits in one task: one insertReplacementText input each, and two undo steps.
      await command('setSelectionRange', 'editor', 5, 5); await settle(page); await heard(page);
      await page.evaluate(() => { command('setRangeText', ['editor', '!', 5, 5, 'end']); command('setRangeText', ['editor', '?', 6, 6, 'end']); });
      await settle(page);
      expect(await state()).toEqual(['hello!?', 7, 7, 'none']);
      expect(only(await heard(page), 'input')).toEqual([['hello!', 6, 6, 'none', 'insertReplacementText', '!', false, false], ['hello!?', 7, 7, 'none', 'insertReplacementText', '?', false, false]]);
      await page.keyboard.press('ControlOrMeta+Z');
      expect(await state()).toEqual(['hello!', 6, 6, 'none']);
      await page.keyboard.press('ControlOrMeta+Z');
      expect(await state()).toEqual(['hello', 5, 5, 'none']);
      await page.keyboard.press('ControlOrMeta+Shift+Z');
      expect(await state()).toEqual(['hello!', 6, 6, 'none']);
      const history = only(await heard(page), 'beforeinput');
      expect(history.map(e => [e[4], e[6]])).toEqual([['historyUndo', true], ['historyUndo', true], ['historyRedo', true]]);
      // The app's beforeinput runs first: one it cancels changes nothing.
      await page.evaluate(() => { window.prevent = e => ['historyUndo', 'insertParagraph', 'insertLineBreak'].includes(e.inputType ?? e[5]); });
      await page.keyboard.press('ControlOrMeta+Z');
      await page.keyboard.press('Enter');
      await settle(page);
      expect(await state()).toEqual(['hello!', 6, 6, 'none']);
      const cancelled = await heard(page);
      expect(only(cancelled, 'input')).toEqual([]);
      expect(only(cancelled, 'beforeinput').map(e => e[4])).toEqual(['historyUndo', expect.stringMatching(/^insert(Paragraph|LineBreak)$/)]);
      await page.evaluate(() => { window.prevent = () => false; });
      // Typing: the platform's input type and data, the selection it left.
      await page.keyboard.type('y'); await settle(page);
      expect(only(await heard(page), 'input')).toEqual([['hello!y', 7, 7, 'none', 'insertText', 'y', false, false]]);
      // Return in a list: the app hears it before the list continues.
      await command('setRangeText', 'editor', '- one', 0, 99, 'end'); await settle(page); await heard(page);
      await page.keyboard.press('Enter'); await settle(page);
      expect(await state()).toEqual(['- one\n- ', 8, 8, 'none']);
      const enter = (await heard(page)).filter(e => e.type !== 'selectionchange');
      expect(enter.map(e => [e.type, e.value, e.start, e.cancelable])).toEqual([['beforeinput', '- one', 5, true], ['input', '- one\n- ', 8, false]]);
      // Each step undoes alone; the typing before the range edits survives.
      await page.keyboard.press('ControlOrMeta+Z');
      expect(await state()).toEqual(['- one', 5, 5, 'none']);
      await page.keyboard.press('ControlOrMeta+Z');
      expect((await state())[0]).toBe('hello!y');
      await page.keyboard.press('ControlOrMeta+Z');
      expect((await state())[0]).toBe('hello!');
      // HTML's refusals, logged; nothing changes.
      await page.evaluate(() => { logs.length = 0; });
      await command('setRangeText', 'editor', 'x', 3, 1);
      expect(await page.evaluate(() => logs.splice(0))).toEqual(['setRangeText "editor" refused: start exceeds end']);
      expect((await state())[0]).toBe('hello!');
      // A caret the app puts in hidden syntax is drawn, and reads back, at its visible edge (LLP 1045 D6).
      await command('setRangeText', 'editor', 'a **b** c', 0, 99, 'end');
      await command('setSelectionRange', 'editor', 3, 3);
      expect([2, 4]).toContain((await state())[1]);
    }, runtime);
  }, 60000);
}

// The editor's element copies the textarea's attributes, `autofocus` included: it is the same field, offered once.
engineTest.chromium('chromium: a Markdown field keeps its autofocus offer when the editor replaces it', async () => {
  await editor('chromium', '', async page => {
    await page.evaluate(async () => {
      const rt = await import('./rt.js');
      const root = rt.h(document.body, 'div', 0, { id: 'exact-root' }, 0);
      rt.mount(() => {
        for (const name of ['pending', 'offered']) {
          const box = rt.h(root, 'div', 0, { id: name + '-box' }, 0);
          box.hidden = name !== 'offered';
          rt.mde(rt.h(box, 'textarea', 0, { id: name, markup: 'markdown', autofocus: 'true' }, 0));
        }
      });
    });
    await page.waitForFunction(() => ['pending', 'offered'].every(id => document.getElementById(id)?.exactMarkup));
    const result = await page.evaluate(async () => {
      const rt = await import('./rt.js');
      const offered = document.getElementById('offered'), pending = document.getElementById('pending');
      const initial = document.activeElement === offered;
      offered.blur();
      rt.commit(() => {});
      const once = document.activeElement === document.body;
      document.getElementById('pending-box').hidden = false;
      rt.commit(() => {});
      const revealed = document.activeElement === pending;
      pending.blur();
      rt.commit(() => {});
      return { initial, once, revealed, done: document.activeElement === document.body };
    });
    expect(result).toEqual({ initial: true, once: true, revealed: true, done: true });
  }, true);
});

// Chrome's input method, through CDP: a composition's events carry the DOM's
// text and selection in source offsets, hidden syntax counted.
engineTest.chromium('chromium: a composition reports source offsets as it composes', async () => {
  for (const runtime of [false, true]) await editor('chromium', 'a **b** c', async (page, field) => {
    const cdp = await page.context().newCDPSession(page);
    await field.focus();
    await page.evaluate(() => command('setSelectionRange', ['editor', 5, 5]));
    await settle(page); await heard(page);
    await cdp.send('Input.imeSetComposition', { text: 'に', selectionStart: 1, selectionEnd: 1 });
    await cdp.send('Input.imeSetComposition', { text: 'にほ', selectionStart: 2, selectionEnd: 2 });
    await settle(page);
    const composing = await heard(page);
    expect(only(composing, 'beforeinput').map(e => [e[4], e[6], e[7]])).toEqual([['insertCompositionText', false, true], ['insertCompositionText', false, true]]);
    expect(only(composing, 'input')).toEqual([['a **bに** c', 6, 6, 'none', 'insertCompositionText', 'に', false, true], ['a **bにほ** c', 7, 7, 'none', 'insertCompositionText', 'にほ', false, true]]);
    // A range edit waits for the composition to end.
    await page.evaluate(() => { logs.length = 0; command('setRangeText', ['editor', 'x', 0, 0]); });
    expect(await page.evaluate(() => logs.splice(0))).toEqual(['setRangeText "editor" refused: text is being composed']);
    await cdp.send('Input.insertText', { text: '日本' });
    await settle(page);
    expect(await field.evaluate(el => [el.value, el.selectionStart, el.selectionEnd])).toEqual(['a **b日本** c', 7, 7]);
    expect(only(await heard(page), 'input').at(-1)).toEqual(['a **b日本** c', 7, 7, 'none', 'insertCompositionText', '日本', false, false]);
    await page.keyboard.press('ControlOrMeta+Z');
    expect(await field.evaluate(el => el.value)).toBe('a **b** c');
  }, runtime);
}, 60000);

engineTest.chromium('chromium: Markdown editing escapes an inert ancestor at an open modal', async () => {
  await editor('chromium', 'hello', async (page, field) => {
    await field.evaluate(el => {
      const ancestor = document.createElement('div'), dialog = document.createElement('dialog');
      el.before(ancestor); ancestor.append(dialog); dialog.append(el); dialog.showModal(); ancestor.inert = true;
      el.focus(); getSelection().selectAllChildren(el);
    });
    await page.keyboard.insertText('edited');
    expect(await field.evaluate(el => el.value)).toBe('edited');
    await page.keyboard.press('ControlOrMeta+Z');
    expect(await field.evaluate(el => el.value)).toBe('hello');
    for (const target of ['editor', 'dialog']) {
      expect(await field.evaluate((el, target) => {
        const blocked = target === 'editor' ? el : el.closest('dialog');
        blocked.inert = true;
        const accepted = el.exactMarkup.format('bold');
        blocked.inert = false;
        return [accepted, el.value];
      }, target)).toEqual([false, 'hello']);
    }
  }, true);
}, 60000);

// A file saved as "UTF-8 with BOM" and decoded with `String::from_utf8` keeps
// U+FEFF; every offset the editor reports counts it.
engineTest.chromium('chromium: a Markdown source that starts with a byte order mark draws and edits as written', async () => {
  const source = '\ufeff# Title\nfirst line\nsecond line';
  await editor('chromium', source, async (page, field) => {
    expect(await field.evaluate(el => el.value)).toBe(source);
    expect(await field.locator('.md-line').allTextContents()).toEqual(['\ufeff# Title', 'first line', 'second line']);
    await field.focus();
    await field.evaluate((el, end) => el.setSelectionRange(end, end), source.length);
    await settle(page);
    await page.keyboard.insertText('!');
    expect(await field.evaluate(el => el.value)).toBe(source + '!');
    expect(only(await heard(page), 'input').map(e => e[0])).toEqual([source + '!']);
  });
}, 60000);

// On a Mac the editor's chords are ⌘'s, and Control with a letter is the text
// system's binding, as in a plain field: Playwright sends each with the AppKit
// command Chrome attaches for a real keyboard (⌃A moveToBeginningOfParagraph:,
// ⌃B moveBackward:, ⌃K deleteToEndOfParagraph:, ⌃Y yank:).
const onAMac = process.platform === 'darwin' ? engineTest.chromium : notApplicable('Control is the command key off a Mac');
onAMac('chromium: on a Mac, Control chords in a Markdown editor are the text bindings', async () => {
  const source = 'alpha beta gamma\nsecond line here';
  await editor('chromium', source, async (page, field) => {
    // The editor reads a moved caret at `selectionchange`, after the key.
    const state = async () => { await settle(page); return field.evaluate(el => [el.value, el.selectionStart, el.selectionEnd]); };
    const at = async (from, to = from) => { await field.evaluate((el, [from, to]) => el.setSelectionRange(from, to), [from, to]); await settle(page); };
    await field.focus();
    await at(16);
    await page.keyboard.press('Control+A');
    expect(await state()).toEqual([source, 0, 0]);
    await at(6, 10);
    await page.keyboard.press('Control+B');
    expect((await state())[0]).toBe(source);
    await at(6, 10);
    await page.keyboard.press('Control+I');
    expect((await state())[0]).toBe(source);
    await at(5);
    await page.keyboard.press('Control+K');
    expect(await state()).toEqual(['alpha\nsecond line here', 5, 5]);
    await at(0);
    await page.keyboard.press('Control+Y');
    expect(await state()).toEqual([' beta gammaalpha\nsecond line here', 11, 11]);
    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+K');
    expect(await state()).toEqual([' beta gammasecond line here', 11, 11]);
    await page.keyboard.press('Control+Z');
    expect((await state())[0]).toBe(' beta gammasecond line here');
    await at(1, 5);
    await page.keyboard.press('Meta+B');
    expect((await state())[0]).toBe(' **beta** gammasecond line here');
  });
}, 60000);
