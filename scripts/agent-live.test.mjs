import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { main } from './agent.mjs';

test('--live selects the Mac wall-clock carrier and preserves the chosen store and input', async () => {
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
    await assert.rejects(main(['web', '--live', 'state'], injected), /--live requires a macOS/);
    await assert.rejects(main(['macos', '--live', '--timing', 'agent', 'state'], injected), /keeps the platform clock/);
  } finally {console.log = output;}
});
