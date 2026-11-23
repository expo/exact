// LLP 1126.004 D5: the Swift of an app's offered controls.
import { test, expect } from 'bun:test';
import { controlWidgetsSource, offeredControls } from './controls.mjs';
import { intentsSource } from './intents.mjs';

const offered = intent => ({ offered: [{ from: '18.0', intent }] });
const compat = { inputs: { controls: [
  { control: { kind: 'open', title: 'Open', symbol: 'house', url: '/' }, digest: 'a', placement: offered('open') },
  { control: { kind: 'go', title: 'Go', symbol: 'tram', tool: 'go', authentication: 'always' }, digest: 'b', conformances: ['LiveActivityIntent'], placement: offered('tool') },
  { control: { kind: 'plain', title: 'Plain', symbol: 'x', tool: 'plain', authentication: 'required' }, digest: 'c', placement: { omitted: 'control-process' } },
] } };

test('only iOS compiles the controls its bake offered', () => {
  expect(offeredControls(compat, 'ios').map(c => c.control.kind)).toEqual(['open', 'go']);
  for (const platform of ['macos', 'tvos', 'watchos']) expect(offeredControls(compat, platform)).toEqual([]);
});

test('the app presses through the door it imports, and the extension draws a button for each', () => {
  const controls = offeredControls(compat, 'ios');
  const app = intentsSource([], {}, controls);
  expect(app).toContain('#if canImport(ExactToolDoor)\nimport ExactToolDoor\n#endif');
  expect(app).toContain('ExactTools.press(control: kind, digest: digest');
  expect(app).toContain('struct ControlOpen_6f70656e: OpenIntent');
  expect(app).toContain('struct ControlPress_676f: LiveActivityIntent');
  expect(app).toContain('.alwaysAllowed');
  expect(app).not.toContain('ControlWidget');
  const { source, members } = controlWidgetsSource('com.example.app', controls);
  expect(source).toContain('StaticControlConfiguration(kind: "com.example.app.control.go")');
  expect(members).toEqual(['if #available(iOS 18.0, *) { Control_6f70656e() }', 'if #available(iOS 18.0, *) { Control_676f() }']);
  // A URL control alone needs no door.
  expect(intentsSource([], {}, controls.slice(0, 1))).not.toContain('import ExactToolDoor');
});

test('a qualification build compiles a recording intent and a plain intent that asks for the app', () => {
  const qualifying = (from, intent, lifted) => ({ offered: [{ from, intent }], qualifying: lifted });
  const controls = offeredControls({ inputs: { controls: [
    { control: { kind: 'r', title: 'Record', symbol: 'mic', tool: 'record', authentication: 'always' }, digest: 'd',
      conformances: ['LiveActivityIntent', 'AudioRecordingIntent'], placement: qualifying('18.0', 'recording', 'control-recording-unqualified') },
    { control: { kind: 'p', title: 'Plain', symbol: 'x', tool: 'plain', authentication: 'required' }, digest: 'c',
      conformances: [], placement: qualifying('26.0', 'tool', 'control-process') },
  ] } }, 'ios');
  const app = intentsSource([], {}, controls);
  expect(app).toContain('@available(iOS 18.0, *)\nstruct ControlPress_72: LiveActivityIntent, AudioRecordingIntent {');
  expect(app).toContain('@available(iOS 26.0, *)\nstruct ControlPress_70: AppIntent {');
  expect(app).toContain('static let supportedModes: IntentModes = .background\n    @available(iOS 27.0, *) static var allowedExecutionTargets: IntentExecutionTargets { .main }');
  expect(app.match(/supportedModes/g)).toHaveLength(1);
});
