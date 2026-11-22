import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { main } from './agent.mjs';
import { open } from './agent-session.mjs';
import { touchTestTarget } from '../host/apple/touches.mjs';

test('--live selects the wall-clock carrier on Apple devices and preserves the chosen store and input', async () => {
  const output = console.log;
  console.log = () => {};
  let options, typed;
  const fake = { state: async () => ({}), type: async (...args) => { typed = args; return {}; }, close: async () => {} };
  const injected = {open: async value => {options = value; return fake;}, driveServer: async () => null,
    exclusiveNativeStore: async (_, launch) => launch(), resolveDevice: () => 'mac'};
  try {
    assert.equal(await main(['macos', '--live', '--storage', 'latency', 'type field hello', 'state'], injected), 0);
    assert.equal(options.env.EXACT_AGENT, 'live');assert.equal(options.timing, 'platform');assert.equal(options.storage, 'latency');
    assert.deepEqual(typed, ['field', 'hello']);
    assert.equal(await main(['ios', '--live', 'state'], injected), 0);
    assert.equal(options.env.EXACT_AGENT, 'live');assert.equal(options.timing, 'platform');
    await assert.rejects(main(['web', '--live', 'state'], injected), /--live requires a macOS, iOS or Android reader/);
    await assert.rejects(main(['android', '--live', '--headless', 'state'], injected), /requires the reader APK/);
    await assert.rejects(main(['android', '--live', '--test', 'app.test.contract'], injected), /without --test/);
    assert.equal(await main(['ios', '--live', '--device', '--touch', 'platform', 'state'], injected), 0);
    assert.equal(options.device, true);
    assert.equal(options.touch, 'platform');
    assert.equal(options.env.EXACT_AGENT, 'live');
    await assert.rejects(main(['macos', '--live', '--timing', 'agent', 'state'], injected), /keeps the platform clock/);
  } finally {console.log = output;}
});

test('a live phone starts the touch carrier for the installed app and keeps the platform clock', async () => {
  let launch;
  const session = await open({ host: 'ios', device: true, env: { EXACT_AGENT: 'live' }, timing: 'platform', touch: 'platform' }, {
    openDevice: async options => {
      launch = options;
      return { host: 'ios', ask: async () => ({ clock: 42 }), close: async () => {} };
    },
  });
  assert.equal(session.now, 42);
  assert.equal(launch.touch, 'platform');
  assert.equal(launch.env.EXACT_AGENT, 'live');
  assert.equal(launch.env.EXACT_AGENT_PRESENTATION, 'native');
  assert.deepEqual(touchTestTarget({ device: launch.device, appId: 'test.pager' }), {
    UseDestinationArtifacts: true, IsUITestBundle: true, TestHostBundleIdentifier: 'com.exact.touches.xctrunner',
    TestBundleDestinationRelativePath: '__TESTHOST__/PlugIns/ExactTouches.xctest', UITargetAppBundleIdentifier: 'test.pager',
  });
  await session.close();
});

test('a background interval waits for both scene changes and ends if the original process dies', async () => {
  const { backgroundIOS, jsonLines } = await import('./agent-wire.mjs');
  const { PassThrough } = await import('node:stream');
  const input = new PassThrough(), output = new PassThrough(), lines = jsonLines(input, output, []);
  const opened = [];
  const activate = async id => {
    opened.push(id);
    setImmediate(() => input.write(JSON.stringify({ pushed: id === 'test.app' ? 'foreground' : 'background' }) + '\n'));
  };
  assert.deepEqual(await backgroundIOS(lines, 0, 'test.app', activate), { backgroundMs: 0, foreground: true });
  const reply = lines.ask({ op: 'tree' });
  input.write('{"nodes":[]}\n');
  assert.deepEqual(await reply, { nodes: [] });
  await assert.rejects(backgroundIOS(lines, 10000, 'test.app', async id => {
    opened.push(id);
    input.write('{"pushed":"background"}\n');
    setImmediate(() => lines.fail('the original app exited'));
  }), /original app exited/);
  assert.deepEqual(opened, ['com.apple.Preferences', 'test.app', 'com.apple.Preferences']);
  input.destroy(); output.destroy();
});
