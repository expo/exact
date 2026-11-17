// @exact/language-model's conformance suite on Apple (LLP 1133.016 §10, §13
// stage 2): the fixture app (`test/fixture`) made with `exact new` outside
// the repo, depending on this package by `file:`, built for macOS or an iOS
// simulator and driven by `exact.mjs agent`.
//
// By default the Apple implementation is bound as in production (`--bind
// language-model`) and talks to Foundation Models on this machine. What the
// suite checks depends on what the machine has, which it prints first:
// with Apple Intelligence on, a create, a constrained prompt's typed value,
// a stream to `complete`, a tool's round trip through the app's handler and
// an abort; otherwise the availability's reason and a create that fails.
// Never a reply's wording.
//
// `--agent` drives the fixture under the agent instead: the Swift agent
// model and §10's drives 7 and 8 on the device's runtime (`agentDrives`),
// with its sessions, responses and tool origins disclosed in `state.extensions`.
//
// usage: bun packages/language-model/test/apple.mjs [--agent] [macos|ios] [--sim <udid>]
import { rmSync } from 'node:fs';
import { dirname } from 'node:path';
import { spawnSync } from 'node:child_process';
import { agentDrives, checker, makeFixture, run } from './fixture.mjs';

const args = process.argv.slice(2), agent = args.includes('--agent');
const host = args.includes('ios') ? 'ios' : 'macos';
const sim = args.includes('--sim') ? args[args.indexOf('--sim') + 1] : null;
const { check, failed } = checker();
const { app } = makeFixture();
const flags = [...(sim ? ['--sim', sim] : []), '--size', '600x900'];
run('bun', ['exact.mjs', host === 'ios' ? 'ios' : 'mac', ...(sim ? ['--sim', sim] : [])], app);

const drive = (ops, bind = false) => {
  const r = spawnSync('bun', ['exact.mjs', 'agent', host, ...flags, ...(bind ? ['--bind', 'language-model'] : []), ...ops], { cwd: app, encoding: 'utf8', maxBuffer: 64 << 20 });
  return { ok: r.status === 0, out: `${r.stdout}\n${r.stderr}`, stdout: r.stdout };
};
const shown = (out, id) => [...out.matchAll(new RegExp(`\\[${id}\\] "((?:[^"\\\\]|\\\\.)*)"`, 'g'))].map(m => m[1]).at(-1) ?? '';

if (agent) {
  const state = 'state extensions', script = v => `prefer extension language-model.responses=${JSON.stringify(v)}`;
  const read = ops => {
    const r = drive(['--json', ...ops]);
    check(`${host} agent`, 'the disclosure drive completes', r.ok, r.out.slice(-3000));
    return r.stdout.split('\n').filter(l => l.startsWith('{')).map(l => JSON.parse(l))
      .filter(v => v['language-model']).map(v => v['language-model'].implementation);
  };
  const verify = (name, ok, data) => check(`${host} agent`, name, ok, JSON.stringify(data));
  const text = 'One 😀 two three', title = 'A private title';
  const [empty, preferred, created, started, streaming, aborted, completed, appended, cloned, retired, destroyed] = read([
    'clock data', state,
    script([{ text, hold: true }, { value: { title } }]), 'prefer extension language-model.rate=100',
    'prefer extension language-model.contextWindow=1024', state,
    'tap create', 'clock data', state,
    'type draft A private prompt.', 'tap stream', 'clock data', state,
    'clock +200', 'clock data', state,
    'tap stop', 'clock data', state,
    'prefer extension language-model.rate=0', 'tap make-title', 'clock data', state,
    'tap append', 'clock data', state, 'tap clone', 'clock data', state,
    ...Array.from({ length: 9 }, () => ['tap make-title', 'clock data']).flat(), state,
    'tap destroy-clone', 'clock data', 'tap destroy', 'clock data', state,
  ]);
  verify('the implementation discloses its empty sessions and responses', empty?.sessions?.length === 0 && empty?.responses?.length === 0, empty);
  verify('preferences disclose the remaining script, rate and window', preferred?.script === 2 && preferred?.rate === 100 && preferred?.contextWindow === 1024, preferred);
  const session = created?.sessions?.[0], response = started?.responses?.[0];
  verify('creation discloses transcript counts and tools', session?.turns === 0 && session?.contextUsage > 0 && session?.contextWindow === 1024 &&
    session?.tools?.join(',') === 'findNotes' && session?.responding === null && session?.overflowed === false, created);
  verify('a stream discloses its session and consumes one script entry', response?.state === 'generating' && response?.text === 0 &&
    response?.session === session?.id && started?.sessions?.[0]?.responding === response?.id && started?.sessions?.[0]?.turns === 1 && started?.script === 1, started);
  verify('polling discloses the streamed text length', streaming?.responses?.[0]?.state === 'generating' &&
    streaming?.responses?.[0]?.text === 'One 😀 '.length, streaming);
  verify('abort discloses the error and frees the session', aborted?.responses?.[0]?.state === 'aborted' && aborted?.responses?.[0]?.error === 'AbortError' &&
    aborted?.sessions?.[0]?.responding === null, aborted);
  verify('a whole prompt discloses completion and its UTF-16 text length', completed?.responses?.some(r => r.state === 'complete' && r.text === JSON.stringify({ title }).length) &&
    completed?.script === 0 && completed?.rate === 0, completed);
  verify('append updates the transcript count and usage', appended?.sessions?.[0]?.turns === 3 && appended?.sessions?.[0]?.contextUsage > completed?.sessions?.[0]?.contextUsage, appended);
  verify('a clone discloses its own session with the copied transcript', cloned?.sessions?.length === 2 && cloned?.sessions?.every(s =>
    s.turns === 3 && s.contextUsage === appended?.sessions?.[0]?.contextUsage && s.responding === null), cloned);
  verify('retirement removes the oldest response records', retired?.responses?.length === 8 && retired?.responses?.every(r =>
    r.state === 'complete' && !aborted?.responses?.some(old => old.id === r.id)), retired);
  verify('destroying sessions clears their disclosed records', destroyed?.sessions?.length === 0 && destroyed?.responses?.length === 0, destroyed);
  const snapshots = [preferred, created, streaming, completed, appended, cloned];
  verify('disclosure keeps prompt, transcript and reply content private', snapshots.every(Boolean) && !JSON.stringify(snapshots).match(
    /You answer in one sentence|A private prompt|A private appended turn|One 😀|A private title/), snapshots);

  const [unreadStarted, unreadPartial, unreadComplete] = read([
    script([{ text }]), 'prefer extension language-model.rate=100',
    'tap create', 'clock data', 'tap stream-unobserved', 'clock data', state,
    'clock +100', state, 'clock +900', state,
  ]);
  verify('a stream without a response reader discloses its initial state', unreadStarted?.responses?.[0]?.state === 'generating' &&
    unreadStarted?.responses?.[0]?.text === 0, unreadStarted);
  verify('the session clock discloses partial text without a response reader', unreadPartial?.responses?.[0]?.state === 'generating' &&
    unreadPartial?.responses?.[0]?.text === 'One '.length, unreadPartial);
  verify('the session clock completes a stream without a response reader', unreadComplete?.responses?.[0]?.state === 'complete' &&
    unreadComplete?.responses?.[0]?.text === text.length && unreadComplete?.sessions?.[0]?.responding === null &&
    unreadComplete?.sessions?.[0]?.contextUsage > unreadStarted?.sessions?.[0]?.contextUsage, unreadComplete);

  const [held, stopped, faster, finished, dropped] = read([
    script([{ text: 'One two', hold: true }, { text }, { text }]), 'prefer extension language-model.rate=100',
    'tap create', 'clock data', 'tap stream-unobserved', 'clock data', 'clock +1000', state,
    'tap stop-unobserved', 'clock data', 'clock +1000', state,
    'tap stream-unobserved', 'clock data', 'clock +100', 'prefer extension language-model.rate=50', state,
    'clock +100', state,
    'tap stream-unobserved', 'clock data', 'tap destroy', 'clock data', 'clock +1000', state,
  ]);
  verify('a held stream without a reader stops before its last token', held?.responses?.[0]?.state === 'generating' &&
    held?.responses?.[0]?.text === 'One '.length, held);
  verify('an abort without a reader cancels further stream updates', stopped?.responses?.[0]?.state === 'aborted' &&
    stopped?.responses?.[0]?.text === 'One '.length && stopped?.sessions?.[0]?.responding === null, stopped);
  verify('a preferred rate reschedules an unread stream', faster?.responses?.at(-1)?.state === 'generating' &&
    faster?.responses?.at(-1)?.text === 'One 😀 '.length && finished?.responses?.at(-1)?.state === 'complete' &&
    finished?.responses?.at(-1)?.text === text.length, [faster, finished]);
  verify('destroying an unread stream cancels its scheduled updates', dropped?.sessions?.length === 0 && dropped?.responses?.length === 0, dropped);

  const [queued, settled, released] = read([
    script([{ text: 'One two' }, { value: { title } }]), 'prefer extension language-model.rate=100',
    'tap create', 'clock data', 'tap stream-unobserved', 'clock data', 'tap make-title',
    'clock until state extensions.language-model.implementation.responses.1.state == "generating" within 5000', state,
    'clock settle', state, 'clock +200', 'clock data', state,
  ]);
  verify('settle reveals no stream tokens while a whole prompt waits behind it', queued?.responses?.length === 2 &&
    settled?.responses?.length === 2 && settled?.responses?.every(r => r.state === 'generating' && r.text === 0) &&
    settled?.script === 1, [queued, settled]);
  verify('advancing time completes the stream and releases its queued prompt', released?.responses?.length === 2 &&
    released?.responses?.every(r => r.state === 'complete') && released?.script === 0, released);

  const fastText = 'x '.repeat(5000);
  const [fast] = read([
    script([{ text: fastText }]), 'prefer extension language-model.rate=0.125',
    'tap create', 'clock data', 'tap stream-unobserved', 'clock data', 'clock +625', state,
  ]);
  verify('a fast unread stream completes in one seek without exhausting hatch callbacks', fast?.responses?.[0]?.state === 'complete' &&
    fast?.responses?.[0]?.text === fastText.length && fast?.sessions?.[0]?.responding === null, fast);

  const [calling, toolDone, cancelled] = read([
    script([{ toolCalls: [{ name: 'findNotes', arguments: { query: 'hold' } }], text: 'A tool reply.' }, { text: 'held', hold: true }]),
    'tap create', 'clock data', 'tap stream',
    'clock until state extensions.language-model.implementation.responses.0.toolCalls.0.state == "running" within 5000', state,
    'clock +1000', 'clock data', 'clock +1000', 'clock data', state,
    'tap stream', 'clock data', 'tap make-title', 'tap make-title', 'clock data', state,
  ]);
  const toolResponse = calling?.responses?.[0], tool = toolResponse?.toolCalls?.[0];
  verify('a running tool discloses its state and origin', toolResponse?.state === 'calling-tool' && tool?.name === 'findNotes' &&
    tool?.state === 'running' && tool?.origin === `tool:model:${toolResponse?.id}`, calling);
  verify('a finished tool keeps its origin and omits its payloads', toolDone?.responses?.[0]?.toolCalls?.[0]?.state === 'done' &&
    toolDone?.responses?.[0]?.toolCalls?.[0]?.origin === tool?.origin &&
    [tool, toolDone?.responses?.[0]?.toolCalls?.[0]].every(t => Object.keys(t ?? {}).sort().join(',') === 'name,origin,state'), toolDone);
  verify('a superseded prompt discloses cancellation', cancelled?.responses?.some(r => r.state === 'aborted' && r.error === 'AbortError') &&
    cancelled?.responses?.some(r => r.state === 'generating'), cancelled);

  const [downloading, ready, failed] = read([
    'prefer extension language-model.languageModelAvailability={"availability":"downloadable"}', 'tap create',
    'clock until state extensions.language-model.implementation.downloading == 0 within 5000', state,
    'prefer extension language-model.languageModelAvailability={"availability":"available"}', 'clock data', state,
    script([{ error: 'NotReadableError', reason: 'refusal' }]), 'tap stream', 'clock data', state,
  ]);
  verify('a download discloses progress before creation', downloading?.downloading === 0 && downloading?.sessions?.length === 0, downloading);
  verify('download completion clears its progress and discloses the session', ready?.downloading === null && ready?.sessions?.length === 1, ready);
  verify('a failed response discloses its error', failed?.responses?.[0]?.state === 'failed' && failed?.responses?.[0]?.error === 'NotReadableError', failed);
  for (const [name, ops, want] of agentDrives()) {
    const { ok, out } = drive(ops);
    const missing = want.filter(w => !out.includes(w));
    check(`${host} agent`, name, ok && !missing.length, `missing ${JSON.stringify(missing)}\n${out.split('\n').slice(-30).join('\n')}`);
  }
} else {
  const first = drive(['clock +500 real', 'clock data', 'tree availability', 'tree tooled'], true);
  const availability = shown(first.out, 'availability');
  console.log(`${host}: availability "${availability}", with tools "${shown(first.out, 'tooled')}"`);
  check(host, 'availability is one of the interface\'s', first.ok && /^(available -|unavailable (device-not-eligible|not-enabled|model-not-ready|os|unsupported-options)|downloadable -|downloading -) /.test(availability), first.out.slice(-2000));
  if (!availability.startsWith('available')) {
    const { ok, out } = drive(['tap create', 'clock data', 'tree created-error'], true);
    check(host, 'a create while unavailable fails', ok && /\S/.test(shown(out, 'created-error')), out.slice(-2000));
  } else {
    const wait = (n) => Array.from({ length: n }, () => ['clock +1000 real', 'clock data']).flat();
    const { ok, out } = drive(['tap ask', 'clock data', ...wait(20), 'tree asked', 'tap create', 'clock data', 'tree created',
      'type draft Use the findNotes tool to look up my notes about lifetimes, then tell me their titles.', 'tap stream', 'clock data', ...wait(30),
      'tree reply-state', 'tree reply', 'tree tool', 'logs'], true);
    // Data code's session has no tools: its constraint is a schema object, its value JSON text.
    const asked = shown(out, 'asked').replace(/\\"/g, '"');
    let title = null;
    try { title = JSON.parse(asked).title; } catch { /* not JSON */ }
    check(host, 'a constrained prompt answers a value of its schema', typeof title === 'string' && title !== '', asked);
    check(host, 'a create answers the session, its window and tools', ok && /^session-\d+ \d+ findNotes false$/.test(shown(out, 'created')), shown(out, 'created') + out.slice(-1500));
    const state = shown(out, 'reply-state'), tool = shown(out, 'tool'), calls = Number(state.split(' ')[1] ?? 0);
    check(host, 'a stream runs to complete', /^complete \d+$/.test(state) && shown(out, 'reply') !== '', `${state} "${shown(out, 'reply')}"`);
    // Whether the model calls the tool is the model's; a call it makes is the app's handler's to answer.
    console.log(`${host}: the model made ${calls} tool calls${calls ? `; the last: ${tool}` : ''}`);
    check(host, 'a tool the model calls runs the app\'s handler', !calls || tool.startsWith('findNotes done'), `${state} tool "${tool}"`);
    const stop = drive(['tap create', 'clock data', 'tap stream', 'clock data', 'clock +300 real', 'tap stop', 'clock data', 'tree reply-state'], true);
    check(host, 'abort ends the response aborted', /^(aborted|complete) /.test(shown(stop.out, 'reply-state')), shown(stop.out, 'reply-state') + stop.out.slice(-1500));
  }
}
rmSync(dirname(app), { recursive: true, force: true });
console.log(failed() ? `${failed()} failed` : 'all passed');
process.exit(failed() ? 1 : 0);
