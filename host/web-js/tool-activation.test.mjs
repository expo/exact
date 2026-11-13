import { test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { webCompiler } from './module.mjs';

// Builds `source` as a JS-rendered app with `intents` in its manifest and returns each route's `data-activate`.
async function activations(source, intents, paths) {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/tool-activation-test-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Tool Activation Test', app: { id: 'test.tools', name: 'Tool Activation Test' }, host: { web: {} }, intents }));
    writeFileSync(resolve(dir, 'app.contract'), source);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'tool-activation-test', '--out', resolve(dir, 'dist'), '--render', 'js'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    return paths.map(path => /data-activate="([a-z]+)"/.exec(readFileSync(resolve(dir, 'dist', path, 'index.html'), 'utf8'))?.[1]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

// LLP 1048.007 D1: a page that would wait for the reader is `idle` where the app offers a tool to a browser's agent,
// which only the runtime registers (LLP 1133.023 D8); a face that leaves out the web door, or a declared activation,
// changes nothing.
test('an undeclared route of an app offering tools on the web is idle', async () => {
  const source = (activate = '') => `routes nav
  home "/" render=build
  other "/other" render=build${activate}

component App
  state n = 0
  tool add writes additive closed "Add one to the count." = add
  action add
    n = n + 1
  view
    main
      text "Count"
      button "Add" press=add
`;
  for (const [doors, activate, expected] of [[undefined, '', ['idle', 'idle']], [['apple'], '', ['interaction', 'interaction']],
    [undefined, ' activate=interaction', ['idle', 'interaction']]]) {
    const intents = { tools: { add: { title: 'Add', ...(doors ? { doors } : {}) } } };
    expect([doors, activate, ...await activations(source(activate), intents, ['', 'other'])]).toEqual([doors, activate, ...expected]);
  }
}, 180000);
