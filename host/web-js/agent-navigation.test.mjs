import { test, expect } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { judge, line } from '../../scripts/agent-check.mjs';
import { largestText } from '../../scripts/verify.mjs';
import { webCompiler } from './module.mjs';

const inPanel = source => source.replace(/^( {6,})/gm, '  $1').replace(/(    main.*)\n/, `$1
      row role="tablist"
        button role="tab" aria-controls="panel" press=bump
          text "Home"
      column role="tabpanel" id="panel"
`);

const afterTitle = content => source => source.replace(/^(          text "Flashcards".*)$/m, `$1\n${content}`);
const subtitle = '          text "Twelve items still left today" width=40 testId="extra-subtitle"';
const glyphLine = `          row testId="glyph-line"
            image "symbol:sf/bell.slash" width=16 height=16
            text "Muted until tomorrow" width=40 testId="extra-glyph"`;
const readouts = `          text "Saved" role="status" width=40 testId="extra-status"
          row role="status" testId="status-row"
            box width=6 height=6 background-color="#ff0000"
            text "Recording this meeting" width=40 testId="extra-readout"`;
const popoverAction = action => source => afterTitle(`          button popovertarget="menu" popovertargetaction="${action}" width=40 testId="extra-popover"
            text "Dismiss menu"`)(source) + `      column popover="auto" id="menu"
        text "Menu"
`;

// The title in a box wider than the viewport at this size, after Create so the header's buttons stay on the screen.
const titleBox = content => source => {
  const title = source.match(/^ {10}text "Flashcards".*$/m)[0];
  return source.replace(`${title}\n`, '').replace(/^( {12}text "Create".*)$/m, `$1
          column width="12rem" flex-shrink=0 testId="title-box"
  ${title}${content}`);
};

const cases = [
  ['subtitle and unplaced content', afterTitle(`          box background-color="#eeeeee"
            text "A badge keeps its face" width=40 testId="extra-badge"
${subtitle}
          text "A later text stays judged" width=40 testId="extra-later"`), ['extra-badge', 'extra-later']],
  ['glyph-line subtitle', afterTitle(glyphLine), []],
  ['glyph line beside authored readout', afterTitle(glyphLine + `
            text "Saved" role="status" appearance="none" width=40 testId="extra-status"`), ['extra-status']],
  ['filled glyph line', afterTitle(glyphLine.replace('row testId=', 'row background-color="#eeeeee" testId=') + `
${subtitle}`), ['extra-glyph']],
  ['status readouts', afterTitle(readouts), []],
  ['authored status readouts', afterTitle(readouts.replaceAll('role="status"', 'role="status" appearance="none"')), ['extra-status', 'extra-readout']],
  ['subtitle under authored heading', s => afterTitle(subtitle)(s).replace('text "Flashcards"', 'text "Flashcards" appearance="none"'), ['title', 'extra-subtitle']],
  ['subtitle outside title group', s => afterTitle(subtitle)(s).replace(/^(          text "Flashcards".*)$/m, '          column\n  $1'), ['extra-subtitle']],
  ['subtitle in authored title group', s => s.replace(/^(          text "Flashcards".*)$/m, `          column press=bump appearance="none"\n  $1\n  ${subtitle}`), ['title', 'extra-subtitle']],
  ['uppercase hide popover action', popoverAction('HIDE'), ['extra-popover']],
  ['mixed-case hide popover action', popoverAction('HiDe'), ['extra-popover']],
  ['untrimmed popover action', popoverAction(' HIDE '), []],
  ['disabled tab outside tablist', afterTitle(`          button role="tab" disabled=true width=40 testId="extra-tab"
            text "Disabled tab"`), ['extra-tab']],
  ['disabled button item', afterTitle(`          button disabled=true width=40 testId="extra-disabled"
            text "Disabled action"`), []],
  ...['button', 'link', 'checkbox', 'radio', 'switch'].map(role => [
    `${role} before subtitle`, afterTitle(`          box role="${role}"
            text "A control keeps its label" width=40 testId="extra-control"
${subtitle}`), ['extra-control'],
  ]),
  ['title in a box wider than the viewport', titleBox(''), []],
  ['title box with an authored readout', titleBox(`
            text "Saved" role="status" appearance="none" width=40 testId="extra-status"`), ['extra-status', 'overflow']],
  ['title box with a control', titleBox(`
            box role="button"
              text "A control keeps its label" width=40 testId="extra-control"`), ['extra-control', 'overflow']],
  ['projected route header', s => s, []],
  ['sibling before navigation root', s => s, ['title', 'cancel', 'create'], 'beforebegin'],
  ['sibling after navigation root', s => s, [], 'afterend'],
  ['route in a tabpanel', inPanel, []],
  ['transparent navigation bar', s => s.replace('column navigationKey="home"', 'column navigationBar="transparent" navigationKey="home"'), []],
  ['explicit automatic navigation bar', s => s.replace('column navigationKey="home"', 'column navigationBar="auto" navigationKey="home"'), []],
  ['navigationBar none', s => s.replace('column navigationKey="home"', 'column navigationBar="none" navigationKey="home"'), ['title', 'cancel', 'create']],
  ['authored header', s => s.replace('header display=', 'header appearance="none" display='), ['title', 'cancel', 'create']],
  ['authored button', s => s.replace('button press=bump width=65', 'button appearance="none" press=bump width=65'), ['cancel']],
  ['pressable title', s => s.replace(/^(          text "Flashcards".*)$/m, '          column press=bump width=130 testId="title-tap"\n  $1'), []],
  ['authored title group', s => s.replace(/^(          text "Flashcards".*)$/m, '          column appearance="none" press=bump width=130 testId="title-tap"\n  $1'), ['title']],
  ['authored title', s => s.replace('text "Flashcards"', 'text "Flashcards" appearance="none"'), ['title']],
  ['ordinary header', s => s.replaceAll('navigationKey="home"', '').replace('navigationBack="back"', ''), ['title', 'cancel', 'create']],
  ['header after body content', s => s.replace('        header display=', '        box height=1\n        header display='), ['title', 'cancel', 'create']],
  ['two headings', s => s.replace('        text "BodyOverflow"', '          text "Other title" role="heading"\n        text "BodyOverflow"'), ['title', 'cancel', 'create']],
];

/** `run(browser)` on `contract` built for the web and opened at a phone's width. */
async function onPhone(contract, run) {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/navigation-agent-test-'));
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Navigation Agent Test', app: { id: 'test.navigation-agent', name: 'Navigation Agent Test' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), contract);
    const dist = resolve(dir, 'dist');
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'navigation-agent-test', '--out', dist, '--render', 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
      const path = new URL(req.url).pathname, file = Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1)));
      return await file.exists() ? new Response(file) : new Response('not found', { status: 404 });
    } });
    browser = await open({ host: 'web', url: server.url.href, size: [390, 844] });
    await run(browser);
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}

for (const [name, source, kept, sibling] of cases) test(`largest-text comparison: ${name}`, () => onPhone(source(`component NavigationAgent
  state count = 0
  action bump
    count = count + 1
  view
    main navigationKey="home" navigationBack="back" testId="navigation"
      column navigationKey="home" testId="route"
        header display="flex" width=320 height=70 testId="header"
          button press=bump width=65 flex-shrink=0 testId="cancel"
            text "Cancel" testId="cancel-label"
          text "Flashcards" role="heading" aria-level=1 text-overflow="clip" overflow="visible" width=130 flex-shrink=0 testId="title"
          button press=bump width=65 flex-shrink=0 testId="create"
            text "Create" testId="create-label"
        text "BodyOverflow" width=60 testId="body"
`), async (browser) => {
  // Contract requires one root; bare plans can give the host several.
  if (sibling) await browser.carrier.evaluate(`document.querySelector('main').insertAdjacentHTML(${JSON.stringify(sibling)}, '<div data-testid="banner" style="height:1px"></div>')`);
  await browser.prefer({ 'root-font-size': 53 });
  const nodes = (await browser.op({ op: 'tree' })).nodes, by = name => nodes.find(n => n.props?.testId === name);
  const raw = judge(await browser.op({ op: 'check' }), null);
  // These web findings are real: only the iOS-sized comparison should omit the projected content.
  const extra = nodes.map(n => n.props?.testId).filter(id => id?.startsWith('extra-'));
  for (const name of ['title', 'cancel-label', 'create-label', 'body', ...extra]) {
    expect(raw.some(f => f.kind === 'clipped' && (f.id === by(name).id || f.of === by(name).id)), name).toBe(true);
  }
  const projected = await largestText(browser, 'navigation');
  expect(projected.error).toBeUndefined();
  const map = new Map(nodes.map(n => [n.id, n]));
  for (const name of ['title', 'cancel', 'create', ...extra]) {
    const clipped = raw.filter(f => f.kind === 'clipped' && (f.id === by(name).id || f.of === by(name).id));
    expect(clipped.some(f => projected.lines.some(l => l.startsWith(line(f, map).split(' [')[0]))), name).toBe(kept.includes(name));
  }
  expect(projected.lines.some(l => l.trimStart().startsWith('overflow ')), 'overflow').toBe(kept.includes('overflow'));
  expect(projected.lines.some(l => l.includes('body'))).toBe(true);
  if (name === 'disabled button item') expect(by('extra-disabled').props.accessibilityRole).toBe('button');
  expect(by('header').props.semanticTag).toBe('header');
  if (name !== 'ordinary header') {
    expect(by('route').props.navigationKey).toBe('home');
    expect(by('navigation').props.navigationBack).toBe('back');
  }
  if (name === 'authored header') expect(by('header').style.appearance).toMatchObject({ value: 'none', source: 'authored' });
  if (name === 'navigationBar none') expect(by('route').props.navigationBar).toBe('none');
}), 120_000);

// Five root tabs, which the web's bar grows with the root font until they no longer fit: UIKit's tab bar keeps its items'
// size, so only an app-drawn bar (`appearance="none"`) is the app's to fix at the largest size (#393).
const fiveTabs = (bar, nested = false) => {
  const panels = `column flex=1 min-height=0 position="relative"${nested ? ' width="100%" height=400' : ''}
  for t in nav.tabs key=t.name
    column role="tabpanel" id=\`panel-\${t.name}\` position="absolute" inset=0
      for e in t.stack key=e.id
        column navigationKey=\`\${e.id}\` position="absolute" inset=0
          text "BodyOverflow" width=60 testId=\`body-\${e.name}\``;
  const indent = (text, by) => text.split('\n').map(l => ' '.repeat(by) + l).join('\n');
  return `routes nav
  tab home "/"
  tab search "/search"
  tab library "/library"
  tab profile "/profile"
  tab settings "/settings"

component NavigationAgent
  action pick(name: string)
    nav = select(nav, name)
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back" width="100%" height="100%" display="flex" flex-direction="column"
${nested ? '' : indent(panels, 6) + '\n'}      row role="tablist"${bar}${nested ? ' flex-wrap="wrap"' : ''} testId="tabs"
${nested ? indent(panels, 8) + '\n' : ''}        for t in nav.tabs key=t.name
          button role="tab" aria-controls=\`panel-\${t.name}\` aria-selected=(nav.tab == t.name) press=pick(t.name) testId=\`tab-\${t.name}\` flex=1
            image "symbol:home"
            text (t.name == "home" ? "Home" : t.name == "search" ? "Search" : t.name == "library" ? "Library" : t.name == "profile" ? "Profile" : "Settings")
`;
};
// A tablist that holds the root's panels keeps them in the page on iOS (NavigationCoversIOS.swift): their routes'
// overflow is still the app's to fix.
for (const [name, bar, kept, nested] of [['the root tab bar', '', false], ['an app-drawn tab bar', ' appearance="none"', true], ['a root tab bar holding its panels', '', false, true]]) test(`largest-text comparison: ${name}`, () => onPhone(fiveTabs(bar, nested), async (browser) => {
  await browser.prefer({ 'root-font-size': 53 });
  const nodes = new Map((await browser.op({ op: 'tree' })).nodes.map(n => [n.id, n]));
  const tabs = new Set([...nodes.values()].filter(n => /^tab-/.test(n.props?.testId)).map(n => n.id));
  const onTabs = (fs) => fs.filter(f => tabs.has(f.id) || tabs.has(f.of)).map(f => line(f, nodes));
  // The web's bar does not fit at this size: check (every size) says so.
  expect(onTabs(judge(await browser.op({ op: 'check' }), null)).length).toBeGreaterThan(0);
  const large = await largestText(browser, 'tabs');
  expect(large.error).toBeUndefined();
  expect(large.lines.some(l => /tab-/.test(l)), large.lines.join('\n')).toBe(kept);
  expect(large.lines.some(l => l.includes('body-home'))).toBe(true);
}), 120_000);
