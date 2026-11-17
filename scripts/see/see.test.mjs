import {test, expect} from 'bun:test';
import {mkdtempSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {encodePng} from '../png.mjs';
import {checkpoint} from './checkpoint.mjs';
import {blind, comparePixels, compareTrees, imageFacts, modelElements, targetPoint, verdict} from './compare.mjs';
import {inputOutcome, internalInput, parse, platformInput, setupInput} from './main.mjs';
import {mainWindow} from './platform.mjs';

const picture = () => ({width: 2, height: 1, data: new Uint8Array([0, 0, 0, 255, 255, 255, 255, 255])});

function displayFixture() {
  const snapshot = {elements: [{identifier: 'ok', label: 'OK', enabled: true, frame: [0, 0, 2, 1]}], frame: [0, 0, 2, 1]};
  const session = {host: 'ios', clock: async () => ({settled: true}),
    tree: async () => ({epoch: 1, incarnation: 1, nodes: [{props: {testId: 'ok', text: 'OK'}}]}),
    state: async () => ({slots: {value: 1}}),
    screenshot: async path => { writeFileSync(path, encodePng(picture())); return {screenshot: path, pixels: 'display', w: 2, h: 1, scale: 1}; }};
  const observer = {identity: {simulator: 'fixture'}, tree: async () => snapshot,
    capture: async path => { writeFileSync(path, encodePng(picture())); return {path, region: 'display'}; }};
  return {session, observer};
}

test('omitted disabled props still compare the enabled state', () => {
  for (const disabled of [undefined, false, 'false', true, 'true']) {
    const enabled = ![true, 'true'].includes(disabled);
    const inside = modelElements({nodes: [{props: {testId: 'ok', disabled}}]});
    expect(compareTrees(inside, [{identifier: 'ok', enabled: !enabled}]).findings).toContainEqual(
      expect.objectContaining({kind: 'tree-field', field: 'enabled', inside: enabled, outside: !enabled}));
    expect(compareTrees(inside, [{identifier: 'ok', enabled}]).findings).toEqual([]);
  }
});

test('platform input refuses changed targets and validates a fresh outside read before posting', async () => {
  const first = {pid: 12, frontmost: true, elements: [
    {identifier: 'ok', enabled: true, frame: [10, 20, 30, 40]},
    {identifier: 'cancel', enabled: true, frame: [100, 20, 30, 40]},
  ]};
  const swapped = {...first, elements: first.elements.map((e, i) => ({...e, frame: first.elements[1 - i].frame}))};
  let posted = 0, reads = 0, latest = first;
  const observer = {tree: async () => { reads++; return latest; },
    input: async (action, point, tree) => { posted++; return {point, tree}; }};
  const action = {kind: 'tap', target: 'ok'};
  await expect(platformInput(observer, action, {outside: first, outsideAfter: swapped})).rejects.toThrow('changed');
  expect(posted).toBe(0);
  latest = swapped;
  await expect(platformInput(observer, action, {outside: first, outsideAfter: first})).rejects.toThrow('changed');
  expect(posted).toBe(0);
  latest = {...first, pid: 13};
  await expect(platformInput(observer, action, {outside: first, outsideAfter: first})).rejects.toThrow('changed');
  expect(posted).toBe(0);
  for (latest of [{...first, frontmost: false}, {...first, errors: ['AX denied']}, {...first, truncated: true}, {error: 'read failed'}]) {
    await expect(platformInput(observer, action, {outside: latest, outsideAfter: latest})).rejects.toThrow();
    expect(posted).toBe(0);
  }
  latest = structuredClone(first);
  const priorReads = reads;
  const result = await platformInput(observer, action, {outside: first, outsideAfter: first});
  expect(reads).toBe(priorReads + 1);
  expect(result.input.point).toEqual([25, 40]);
  expect(result.input.tree).toBe(latest);
  expect(posted).toBe(1);
});

test('failed tap and setup receipts abort before proceeding', async () => {
  for (const failure of [{error: 'modal blocks view', delivery: 'recognized'}, {delivery: 'unsupported', reason: 'no touch'},
    {settled: false, reason: 'still moving'}, {landError: 'landing failed'}]) {
    let taps = 0, clocks = 0;
    const session = {tap: async () => { taps++; return failure; }, clock: async () => { clocks++; }};
    await expect(internalInput(session, {kind: 'tap', target: 'ok'})).rejects.toThrow();
    taps = 0;
    await expect(setupInput(session, ['ok', 'next'])).rejects.toThrow();
    expect(taps).toBe(1);
    expect(clocks).toBe(0);
  }
});

test('drag rejects every failing phase and releases accepted contacts', async () => {
  const action = {kind: 'drag', target: 'ok', dx: 0, dy: 100, ms: 300};
  for (const phase of ['down', 'move', 'up']) for (const failure of [
    {error: 'blocked'}, {delivery: 'unsupported', reason: 'not supported'}, {settled: false}, {landError: 'landing failed'},
  ]) {
    const calls = [];
    const reply = async name => { calls.push(name); return name === phase ? failure : {delivery: 'recognized'}; };
    const session = {tap: () => reply('down'), pointer: reply};
    await expect(internalInput(session, action)).rejects.toThrow(phase);
    const noContact = phase === 'down' && (failure.error || failure.delivery === 'unsupported');
    expect(calls).toEqual(noContact ? ['down'] : phase === 'down' ? ['down', 'up'] : ['down', 'move', 'up']);
  }
  for (const phase of ['down', 'move']) {
    const calls = [];
    const reply = async name => { calls.push(name); if (name === phase) throw new Error('transport closed'); return {delivery: 'recognized'}; };
    await expect(internalInput({tap: () => reply('down'), pointer: reply}, action)).rejects.toThrow('transport closed');
    expect(calls).toEqual(phase === 'down' ? ['down'] : ['down', 'move', 'up']);
  }
  const receipt = {delivery: 'recognized'};
  expect(await internalInput({tap: async () => receipt, pointer: async () => receipt}, action)).toEqual(
    {down: receipt, move: receipt, up: receipt});
});

test('missing outside identifiers never aim through a model label, even when unique', async () => {
  for (const duplicate of [false, true]) {
    const model = {nodes: [{props: {testId: 'missing', text: 'Save'}},
      ...(duplicate ? [{props: {testId: 'present', text: 'Save'}}] : [])]};
    const outside = {elements: [{identifier: 'present', label: 'Save', frame: [10, 20, 30, 40]}]};
    let posts = 0;
    const observer = {tree: async () => outside, input: async () => { posts++; return {}; }};
    await expect(platformInput(observer, {kind: 'tap', target: 'missing'},
      {model, outside, outsideAfter: outside})).rejects.toThrow('0 matches');
    expect(posts).toBe(0);
  }
});

test('name joins require both elements to lack identifiers', () => {
  for (const [inside, outside] of [
    [[{identifier: 'a', label: 'Save'}], [{identifier: 'b', label: 'Save'}]],
    [[{identifier: 'a', label: 'Save'}], [{label: 'Save'}]],
    [[{label: 'Save'}], [{identifier: 'b', label: 'Save'}]],
  ]) {
    const result = compareTrees(inside, outside);
    expect(result.joins).toEqual([]);
    expect(result.findings.map(f => f.kind)).toEqual(['inside-only', 'outside-only']);
  }
  const result = compareTrees([{identifier: 'a', label: 'Save'}, {label: 'Save'}],
    [{identifier: 'a', label: 'Save'}, {label: 'Save'}]);
  expect(result.joins.map(j => j.via)).toEqual(['identifier', 'name-only']);
  expect(result.findings).toEqual([]);
  expect(compareTrees([{label: 'Save'}, {label: 'Save'}], [{label: 'Save'}]).joins).toEqual([]);
});

test('failed setup settlement aborts before the next tap', async () => {
  for (const failure of [{settled: false, reason: 'work refused'}, {error: 'clock failed'}, {landError: 'landing failed'}]) {
    const calls = [];
    const session = {tap: async target => { calls.push(target); return {delivery: 'recognized'}; },
      clock: async () => { calls.push('settle'); return failure; }};
    await expect(setupInput(session, ['first', 'second'])).rejects.toThrow('setup/settle');
    expect(calls).toEqual(['first', 'settle']);
  }
});

test('drag cleanup owns its contact and preserves the original phase failure', async () => {
  const action = {kind: 'drag', target: 'ok', dx: 0, dy: 100, ms: 300};
  for (const failure of [{delivery: 'unsupported', reason: 'no held contact'}, {error: 'down failed'}, new Error('down transport failed')]) {
    const calls = [];
    const session = {tap: async () => { calls.push('down'); if (failure instanceof Error) throw failure; return failure; },
      pointer: async phase => { calls.push(phase); throw new Error('released another contact'); }};
    await expect(internalInput(session, action)).rejects.toThrow(/drag\/down|down transport failed/);
    expect(calls).toEqual(['down']);
  }
  const calls = [];
  const session = {tap: async () => ({delivery: 'recognized'}),
    pointer: async phase => { calls.push(phase); throw new Error(`${phase} transport failed`); }};
  await expect(internalInput(session, action)).rejects.toThrow('move transport failed');
  expect(calls).toEqual(['move', 'up']);
});

test('cross-leg outcomes omit focus but retain keyboard and checkpoint focus stability', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'see-focus-'));
  try {
    const {session, observer} = displayFixture();
    session.state = async () => ({navigation: '/', focus: {logical: 12}, keyboard: {visible: false}});
    const first = await checkpoint(session, observer, resolve(dir, 'first'));
    session.state = async () => ({navigation: '/', focus: {logical: 47, events: ['resign']}, keyboard: {visible: false}});
    const second = await checkpoint(session, observer, resolve(dir, 'second'));
    expect(first.report.status).toBe('agreement');
    expect(second.report.status).toBe('agreement');
    expect(inputOutcome(first)).toEqual(inputOutcome(second));
    expect(inputOutcome({...second, state: {...second.state, keyboard: {visible: true}}})).not.toEqual(inputOutcome(first));
    let focus = 0;
    session.state = async () => ({focus: {logical: focus++}});
    const changing = await checkpoint(session, observer, resolve(dir, 'changing'));
    expect(changing.report.status).toBe('inconclusive');
    expect(changing.report.findings).toContainEqual(expect.objectContaining({source: 'driver/state', kind: 'blind-spot'}));
  } finally { rmSync(dir, {recursive: true, force: true}); }
});

test('fresh input observations retain identity, read interval and raw evidence on refusal', async () => {
  const outside = {pid: 12, elements: [{identifier: 'ok', frame: [10, 20, 30, 40]}]};
  for (const fresh of [{...outside, pid: 13}, {...outside, truncated: true}, {error: 'AX failed'}, new Error('AX transport failed'), outside]) {
    const record = {};
    const observer = {identity: {host: 'macos', pid: 12},
      tree: async () => { if (fresh instanceof Error) throw fresh; return fresh; }, input: async () => ({delivery: 'CGEvent/HID'})};
    const start = Date.now();
    try { await platformInput(observer, {kind: 'tap', target: 'ok'}, {outside, outsideAfter: outside}, record); }
    catch {}
    const observation = JSON.parse(JSON.stringify(record)).inputObservation;
    expect(observation).toMatchObject({source: 'platform/accessibility', identity: observer.identity});
    expect(observation.start).toBeGreaterThanOrEqual(start);
    expect(observation.end).toBeGreaterThanOrEqual(observation.start);
    expect(observation.end).toBeLessThanOrEqual(Date.now());
    if (!(fresh instanceof Error)) expect(observation.result).toEqual(fresh);
    if (fresh !== outside) expect(observation.error).toBeString();
    else expect(observation.error).toBeUndefined();
  }
});

test.skipIf(process.platform !== 'darwin')('Mac gesture refuses a failed release and preserves a failed move', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'see-release-'));
  try {
    const source = readFileSync(resolve(import.meta.dir, 'Mac.swift'), 'utf8').split('\ndo {')[0];
    const harness = `
var cases: [[String: Any]] = []
for failure in ["none", "down", "move", "up"] {
    var events: [String] = []
    do {
        let receipt = try gesture([0, 0, 1, 1, 1], event: { kind, _ in
            let name = kind == .leftMouseDown ? "down" : kind == .leftMouseDragged ? "move" : kind == .leftMouseUp ? "up" : "position"
            events.append(name)
            if name == failure || failure == "move" && name == "up" { throw Refusal(reason: "injected \\(name) failure") }
        }, wait: { _ in })
        cases.append(["case": failure, "events": events, "receipt": receipt])
    } catch {
        cases.append(["case": failure, "events": events, "error": (error as? Refusal)?.reason ?? String(describing: error)])
    }
}
print(String(data: try JSONSerialization.data(withJSONObject: cases), encoding: .utf8)!)
`;
    const file = resolve(dir, 'main.swift'), binary = resolve(dir, 'gesture-tests');
    writeFileSync(file, source + harness);
    const compile = spawnSync('xcrun', ['swiftc', file, '-o', binary], {encoding: 'utf8', timeout: 20000});
    expect(compile.stderr).toBe('');
    expect(compile.status).toBe(0);
    const run = spawnSync(binary, [], {encoding: 'utf8', timeout: 5000});
    expect(run.status).toBe(0);
    const [success, down, move, up] = JSON.parse(run.stdout);
    expect(success.receipt.delivery).toBe('CGEvent/HID');
    expect(success.events.at(-1)).toBe('up');
    expect(down).toMatchObject({error: 'injected down failure', events: ['position', 'down']});
    expect(move.error).toBe('injected move failure');
    expect(move.events.at(-1)).toBe('up');
    expect(up.error).toBe('injected up failure');
    expect(up.receipt).toBeUndefined();
  } finally { rmSync(dir, {recursive: true, force: true}); }
}, 30000);

test('capture motion and failed correlation cannot pass even when all sampled pixels match', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'see-signals-'));
  try {
    for (const window of [false, true]) for (const signal of [{moving: true}, {settled: false},
      {pixelsWhy: 'the display changed while it was read'}, {pixels: 'unavailable', pixelsWhy: 'capture could not be correlated'}]) {
      const {session, observer} = displayFixture();
      const screenshot = session.screenshot;
      session.screenshot = async (path, isWindow) => ({...await screenshot(path), ...(!!isWindow === window ? signal : {})});
      const result = await checkpoint(session, observer, resolve(dir, `${window}-${Object.keys(signal).join('-')}-${signal.pixels ?? ''}`));
      expect(result.report.status).toBe('inconclusive');
      expect(result.report.findings).toContainEqual(expect.objectContaining({kind: 'blind-spot',
        source: window ? 'driver/screenshot-window' : 'driver/screenshot-default'}));
    }
  } finally { rmSync(dir, {recursive: true, force: true}); }
});

test('every unpaired semantic element and differing comparable field is retained', () => {
  const r = compareTrees([{identifier: 'a', label: 'Old', enabled: true}, {label: 'inside'}],
    [{identifier: 'a', label: 'New', enabled: false}, {label: 'outside'}]);
  expect(r.findings.map(f => f.kind)).toEqual(['tree-field', 'tree-field', 'inside-only', 'outside-only']);
  expect(compareTrees([{label: 'same'}], [{label: 'same'}]).joins[0].via).toBe('name-only');
  expect(compareTrees([{identifier: 'a', label: 'OK'}], [{identifier: 'a'}]).findings[0]).toMatchObject({kind: 'tree-field', outside: ''});
  expect(compareTrees([{identifier: 'a'}], [{identifier: 'a'}, {identifier: 'a'}]).findings.some(f => f.kind === 'blind-spot')).toBe(true);
});

test('one changed pixel counts; dimensions and blank pictures cannot be blessed', () => {
  const a = picture(), b = picture();
  b.data[0] = 1;
  expect(comparePixels(a, b, 'test')[0]).toMatchObject({changed: 1, bounds: [0, 0, 1, 1]});
  expect(comparePixels(a, a, 'test')).toEqual([]);
  expect(verdict(comparePixels(a, {...b, width: 1}, 'test'))).toBe('inconclusive');
  expect(imageFacts({...a, data: new Uint8Array(8)}).uniform).toBe(true);
  expect(verdict([blind('test', 'permission denied')])).toBe('inconclusive');
});

test('outside aiming never falls back to model coordinates', () => {
  expect(() => targetPoint({elements: []}, 'button', {nodes: [{props: {testId: 'button'}, frame: [0, 0, 10, 10]}]})).toThrow('0 matches');
  const tree = {elements: [{identifier: 'button', frame: [10, 20, 30, 40]}]};
  expect(targetPoint(tree, 'button', {}).point).toEqual([25, 40]);
  expect(() => targetPoint({elements: [...tree.elements, ...tree.elements]}, 'button', {})).toThrow('2 matches');
});

test('window capture requires one external main-window match, regardless of window size', () => {
  const mainFrame = [10, 20, 300, 400], main = {id: 1, frame: mainFrame};
  expect(mainWindow({mainFrame, windows: [main, {id: 2, frame: [0, 0, 900, 900]}]})).toEqual(main);
  expect(() => mainWindow({mainFrame, windows: [main, {...main, id: 2}]})).toThrow('ambiguous');
  expect(() => mainWindow({windows: [main]})).toThrow('unknown');
});

test('checkpoint keeps independent evidence and fails closed on motion, blank capture and permission denial', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'see-test-'));
  try {
    let reads = 0;
    const session = {host: 'macos', clock: async () => ({settled: true}),
      tree: async () => ({nodes: [{id: 1, props: {testId: 'button', text: 'OK'}}]}),
      state: async () => ({slots: {revision: reads++}}),
      screenshot: async path => { writeFileSync(path, encodePng(picture())); return {screenshot: path}; }};
    const observer = {identity: {pid: 12},
      tree: async () => ({elements: [{identifier: 'button', label: 'OK'}]}),
      capture: async path => { writeFileSync(path, encodePng({...picture(), data: new Uint8Array(8)})); return {path}; }};
    const r = await checkpoint(session, observer, resolve(dir, 'moving'));
    expect(r.report.status).toBe('inconclusive');
    expect(r.report.findings.some(f => f.source === 'driver/state')).toBe(true);
    expect(r.report.findings.filter(f => f.reason?.includes('uniform')).length).toBe(2);
    observer.tree = async () => { throw new Error('AX denied'); };
    const denied = await checkpoint(session, observer, resolve(dir, 'denied'));
    expect(denied.report.findings.filter(f => f.reason?.includes('AX denied')).length).toBe(2);
    expect(JSON.parse(readFileSync(resolve(dir, 'denied/outside-before.json'))).error).toBe('AX denied');
    expect(denied.model.nodes).toHaveLength(1);
  } finally { rmSync(dir, {recursive: true, force: true}); }
});

test('CLI rejects unknown flags, missing simulator and invalid gestures', () => {
  expect(parse(['--help']).help).toBe(true);
  expect(() => parse(['ios', '--app', 'shelf', '--out', 'x'])).toThrow('--sim');
  expect(() => parse(['macos', '--app', 'shelf', '--out', 'x', '--typo'])).toThrow('unknown');
  expect(() => parse(['macos', '--app', 'shelf', '--out', 'x', '--drag', 'row', '0', '0', '300'])).toThrow('nonzero');
});

test('a declared full-display capture permits agreement, while a changed display is a finding', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'see-display-'));
  try {
    const {session, observer} = displayFixture();
    expect((await checkpoint(session, observer, resolve(dir, 'same'))).report.status).toBe('agreement');
    const screenshot = session.screenshot;
    session.screenshot = async path => ({...await screenshot(path), pixelsWhy: 'the display as it is, asked for'});
    expect((await checkpoint(session, observer, resolve(dir, 'display-requested'))).report.status).toBe('agreement');
    observer.capture = async path => { const image = picture(); image.data[0] = 2; writeFileSync(path, encodePng(image)); return {path, region: 'display'}; };
    const changed = await checkpoint(session, observer, resolve(dir, 'changed'));
    expect(changed.report.status).toBe('disagreement');
    expect(changed.report.findings.filter(f => f.kind === 'pixels')).toHaveLength(2);
  } finally { rmSync(dir, {recursive: true, force: true}); }
});

test('removal inventory contains every experiment file and every outside hook', () => {
  const root = resolve(import.meta.dir, '../..');
  const removal = readFileSync(resolve(import.meta.dir, 'REMOVING.md'), 'utf8');
  const listed = [...removal.matchAll(/^- `(scripts\/see\/[^`]+)`/gm)].map(m => m[1]).sort();
  const git = spawnSync('git', ['ls-files', '--cached', '--others', '--exclude-standard', 'scripts/see'], {cwd: root, encoding: 'utf8'});
  expect(git.status).toBe(0);
  expect([...new Set(git.stdout.trim().split('\n'))].sort()).toEqual(listed);
  const tracked = spawnSync('git', ['grep', '-l', '-E', 'see: removable|[./]see/', '--', ':!scripts/see', ':!llp'], {cwd: root, encoding: 'utf8'});
  expect(tracked.stdout.trim().split('\n')).toEqual(['scripts/exact.mjs']);
  expect(removal).toContain('`scripts/exact.mjs`');
  const entry = readFileSync(resolve(root, 'scripts/exact.mjs'), 'utf8');
  const hook = entry.match(/\/\/ see: removable \(LLP 1172\.021\)\nif \(import\.meta\.main && process\.argv\[2\] === 'see'\) \{\n  process\.exit\(spawnSync[^\n]+\n\}/g);
  expect(hook).toHaveLength(1);
});
