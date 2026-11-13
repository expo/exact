import { test, expect } from 'bun:test';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { open } from '../../../scripts/agent.mjs';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { serveBuildTree } from '../serve.mjs';

// The wasm fixture needs Markdown linked: EXACT_WEB_LINK=all bun host/web/build.mjs caltrain-web --wasm.
const ROOT = resolve(import.meta.dir, '../../..');
const target = resolve(process.env.CARGO_TARGET_DIR ?? resolve(ROOT, 'target'));
const wasmDist = resolve(process.env.EXACT_WEB_DIST ?? resolve(ROOT, 'host/web/dist'));
const SOURCE = `routes nav
  home "/" render=build activate=interaction
    item "/item" render=build activate=interaction
    explicit "/explicit" render=build activate=interaction
    empty "/empty" render=build activate=interaction
    bare "/bare" render=build activate=interaction
    formatted "/formatted" render=build activate=interaction
    sheet "/sheet" render=build activate=interaction

component App
  state presses = 0
  state menuShown = true
  state dismissToOutside = false
  action back
    nav = back(nav)
  action sheet
    nav = go(nav, "/sheet")
  action choose
    presses = presses + 1
  action removeMenu
    menuShown = false
    if dismissToOutside
      focus("outside")
  action resetMenu
    menuShown = true
    dismissToOutside = true
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back" testId="navigation" width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=\`\${e.id}\` navigationPresentation=(e.name == "sheet" ? "modal" : "push") position="absolute" inset=0 testId=e.name
          if e.name != "bare"
            header
              if e.name == "formatted"
                text "**Inbox** & [Archive](/archive)" markup="markdown" role="heading" aria-level=1
              else
                text (e.name == "home" ? "Home" : "Item & <Detail>") role="heading" aria-level=1
          if e.name == "explicit"
            head title="Authored"
          if e.name == "empty"
            head title=""
          if e.name == "sheet"
            button press=back testId="close"
              text "Close"
          else
            button press=sheet testId="open-sheet"
              text "Open sheet"
            button popovertarget="actions" testId="open-menu"
              text "Actions"
            button id="outside" press=resetMenu testId="outside"
              text "Outside"
            input id="draft" aria-label="Draft" testId="draft"
            row id="context-source" contextPopover="actions"
              text "Context source"
            if menuShown
              column id="actions" popover="auto" role="menu"
                button disabled=true role="menuitem"
                  text "Disabled"
                button press=choose role="menuitem" testId="first" popovertarget="actions" popovertargetaction="hide"
                  text "First"
                button disabled=true role="menuitem"
                  text "Disabled between"
                button press=removeMenu role="menuitem" testId="last"
                  text "Last"
`;

function run(args) {
  const result = spawnSync(args[0], args.slice(1), { cwd: ROOT, encoding: 'utf8' });
  expect(result.status, result.stdout + result.stderr).toBe(0);
  return result.stdout;
}

for (const runtime of ['wasm', 'js']) test(`${runtime}: rendered route titles, modal exposure and keyboard menus`, async () => {
  const dir = mkdtempSync(resolve(target, 'route-pages-')), source = resolve(dir, 'app.contract'), plan = resolve(dir, 'app.plan');
  let session, suite, server;
  try {
    writeFileSync(source, SOURCE);
    run([resolve(target, 'debug/contract'), 'build', source, '-o', plan]);
    const dist = runtime === 'wasm' ? wasmDist : resolve(dir, 'dist');
    if (runtime === 'js') run([process.execPath, 'host/web-js/build.mjs', 'caltrain', '--contract', source, '--out', dist, '--render', 'js']);
    suite = await suiteBrowser();
    const staticPage = await suite.newPage({ javaScriptEnabled: false });
    for (const [path, title] of [['/', 'Home'], ['/item', 'Item & <Detail>'], ['/explicit', 'Authored'], ['/empty', ''], ['/bare', 'Caltrain'], ['/formatted', 'Inbox & Archive']]) {
      const html = runtime === 'wasm' ? JSON.parse(run([resolve(target, 'debug/caltrain-render'), '--plan', plan, '--name', 'Caltrain',
        '--shell', resolve(ROOT, 'host/web/index.html'), path]).trim().split('\n').at(-1)).page
        : readFileSync(resolve(dist, path.slice(1), 'index.html'), 'utf8');
      await staticPage.setContent(html);
      expect(await staticPage.title(), `${runtime} ${path}`).toBe(title);
      expect(await staticPage.locator('meta[property="og:title"]').getAttribute('content')).toBe(title);
      expect(await staticPage.locator('meta[name="twitter:title"]').getAttribute('content')).toBe(title);
    }
    // A reader's page, without the agent: the menu's invoker has no handler, so only its opening starts the runtime.
    if (runtime === 'js') server = createServer((request, response) => serveBuildTree(dist, request, response));
    else {
      // No wasm is built for this plan: the rendered page and the host's own scripts, with a glue.js that only installs
      // the menu rule, as the wasm host's glue does after its first paint.
      const html = JSON.parse(run([resolve(target, 'debug/caltrain-render'), '--plan', plan, '--name', 'Caltrain',
        '--shell', resolve(ROOT, 'host/web/index.html'), '/']).trim().split('\n').at(-1)).page;
      const glue = 'import "./menu-rows.js"; globalThis.exact.menuRows(); globalThis.exact.glued = true;';
      server = createServer((request, response) => {
        const path = new URL(request.url, 'http://localhost').pathname, file = resolve(ROOT, 'host/web', `.${path}`);
        const body = path === '/' ? html : path === '/glue.js' ? glue : /^\/[\w/-]+\.js$/.test(path) && existsSync(file) ? readFileSync(file) : null;
        response.writeHead(body === null ? 404 : 200, { 'content-type': path === '/' ? 'text/html; charset=utf-8' : 'text/javascript' });
        response.end(body ?? '');
      });
    }
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    const reader = await suite.newPage();
    await reader.goto(`http://127.0.0.1:${server.address().port}/`, { waitUntil: 'load' });
    const loaded = () => reader.evaluate(() => !!(globalThis.exact?.ready ?? globalThis.exact?.glued));
    const focusedOn = want => reader.waitForFunction(want => document.activeElement?.dataset.testid === want, want);
    await reader.waitForFunction(() => document.querySelector('script[type="application/vnd.exact.checkpoint"]')?.dataset.activate === 'interaction');
    expect(await loaded()).toBe(false);
    await reader.focus('[data-testid="open-menu"]');
    await reader.keyboard.press('Enter');
    await focusedOn('first');
    expect(await loaded()).toBe(true);
    // The wasm host's arrows are input-glue.js's, which this glue lacks; the agent's drive below presses them.
    if (runtime === 'js') {
      await reader.keyboard.press('ArrowDown');
      await focusedOn('last');
    }
    await reader.keyboard.press('Escape');
    await focusedOn('open-menu');
    expect(await reader.evaluate(() => document.querySelector('#actions').matches(':popover-open'))).toBe(false);
    if (runtime === 'js') session = await open({ host: 'web', url: `http://127.0.0.1:${server.address().port}/`, size: [600, 800] });
    else session = await open({ host: 'web', plan, webDist: dist, size: [600, 800] });
    await session.clock('settle');
    const evaluate = expression => session.carrier.evaluate(expression);
    const key = async (key, windowsVirtualKeyCode, text) => {
      for (const type of ['keyDown', 'keyUp']) await session.carrier.call('Input.dispatchKeyEvent', {
        type, key, code: key, windowsVirtualKeyCode, ...(type === 'keyDown' && text ? { text } : {}),
      });
    };
    const focused = () => evaluate('document.activeElement.dataset.testid');
    await evaluate(`document.querySelector('[data-testid="open-menu"]').focus()`);
    await key('Enter', 13, '\r');
    await session.clock('settle');
    expect(await focused()).toBe('first');
    await key('ArrowDown', 40);
    expect(await focused()).toBe('last');
    await key('ArrowDown', 40);
    expect(await focused()).toBe('first');
    await key('Escape', 27);
    await session.clock('settle');
    expect(await focused()).toBe('open-menu');
    expect(await evaluate('document.querySelector("#actions").matches(":popover-open")')).toBe(false);
    expect((await session.state()).slots.presses).toBe(0);
    await key('Enter', 13, '\r');
    await session.clock('settle');
    await key('Enter', 13, '\r');
    await session.clock('settle');
    expect((await session.state()).slots.presses).toBe(1);
    expect(await focused()).toBe('open-menu');

    // A context menu's source need not be focusable; return to the saved field instead.
    await evaluate(`{
      document.getElementById('draft').focus();
      window.keptMenu = document.getElementById('actions');
      window.menuParent = keptMenu.parentElement;
      window.menuNext = keptMenu.nextSibling;
      keptMenu.showPopover({ source: document.getElementById('context-source') });
    }`);
    await session.clock('settle');
    expect(await focused()).toBe('first');
    await evaluate('keptMenu.remove()');
    await session.clock('settle');
    expect(await focused()).toBe('draft');
    await evaluate('menuParent.insertBefore(keptMenu, menuNext)');

    // Removing an open menu dismisses it without a toggle event.
    for (const want of ['open-menu', 'outside']) {
      await evaluate(`document.querySelector('[data-testid="open-menu"]').focus()`);
      await key('Enter', 13, '\r');
      await session.clock('settle');
      expect(await focused()).toBe('first');
      await key('ArrowDown', 40);
      await key('Enter', 13, '\r');
      await session.clock('settle');
      expect(await evaluate('document.querySelector("#actions")')).toBe(null);
      expect(await focused()).toBe(want);
      if (want === 'open-menu') await session.tap('outside');
    }

    await session.tap('open-sheet');
    await session.clock('settle');
    let tree = await session.tree(null, { ax: true });
    expect(tree.ax.modal.present).toBe(true);
    expect(tree.ax.elements.some(e => e.testId === 'navigation')).toBe(true);
    expect(tree.ax.findings.filter(f => f.kind === 'outside-modal')).toEqual([]);
    // Expose the covered page through the DOM while the runner still declares the sheet.
    await evaluate(`{ const page = document.querySelector('[data-testid="home"]'); page.inert = false; page.style.visibility = 'visible'; }`);
    tree = await session.tree(null, { ax: true });
    expect(tree.ax.findings.filter(f => f.kind === 'outside-modal').some(f => f.testId === 'outside')).toBe(true);
    expect(tree.ax.findings.filter(f => f.kind === 'outside-modal').some(f => f.testId === 'navigation')).toBe(false);
  } finally {
    await session?.close(); await suite?.close(); server?.close();
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);
