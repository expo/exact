import {mkdirSync} from 'node:fs';
import {dirname, resolve} from 'node:path';
import {checkpoint, json, modelState} from './checkpoint.mjs';
import {blind, digest, externalElements, modelElements, targetPoint, verdict} from './compare.mjs';
import {command, macHelper, platform} from './platform.mjs';

export function parse(argv) {
  if (!argv.length || argv.includes('--help')) return {help: true};
  const options = {host: argv[0], setup: []};
  if (!['macos', 'ios'].includes(options.host)) throw new Error('see: host must be macos or ios');
  const args = argv.slice(1);
  const take = flag => { const value = args.shift(); if (!value || value.startsWith('--')) throw new Error(`${flag} needs a value`); return value; };
  while (args.length) {
    const flag = args.shift();
    if (['--app', '--out', '--sim'].includes(flag)) options[flag.slice(2)] = take(flag);
    else if (flag === '--setup-tap') options.setup.push(take(flag));
    else if (flag === '--tap' || flag === '--drag') {
      if (options.action) throw new Error('see: one input per comparison');
      options.action = {kind: flag.slice(2), target: take(flag)};
      if (flag === '--drag') {
        Object.assign(options.action, {dx: Number(take(flag)), dy: Number(take(flag)), ms: Number(take(flag))});
        const {dx, dy, ms} = options.action;
        if (![dx, dy, ms].every(Number.isFinite) || !Math.hypot(dx, dy) || ms <= 0 || ms > 2000)
          throw new Error('see: drag needs nonzero finite travel and 1–2000 milliseconds');
      }
    } else throw new Error(`see: unknown option ${flag}`);
  }
  if (!options.app || !options.out) throw new Error('see: --app and --out are required');
  if (options.host === 'ios' && !options.sim) throw new Error('see: iOS requires --sim naming your booted simulator');
  return options;
}

function delivered(reply, phase) {
  if (!reply || reply.error != null || reply.delivery === 'unsupported' || reply.settled === false || reply.landError != null)
    throw new Error(`${phase}: ${reply?.error ?? reply?.landError ?? reply?.reason ?? 'input was not delivered'}`);
  return reply;
}

export async function internalInput(session, action) {
  if (action.kind === 'tap') return delivered(await session.tap(action.target), 'tap');
  let down, move, up, contact = false, failure;
  try {
    down = await session.tap(action.target, {down: true});
    // Settlement can fail after the down has booked a contact.
    contact = !!down && down.delivery !== 'unsupported' && down.error == null && down.contact !== false;
    delivered(down, 'drag/down');
    move = delivered(await session.pointer('move', {dx: action.dx, dy: action.dy, ms: action.ms, virtual: true}), 'drag/move');
  } catch (error) { failure = error; }
  if (contact) {
    try { up = delivered(await session.pointer('up'), 'drag/up'); }
    catch (error) { failure ??= error; }
  }
  if (failure) throw failure;
  return {down, move, up};
}

export async function setupInput(session, targets) {
  for (const target of targets) {
    await internalInput(session, {kind: 'tap', target});
    delivered(await session.clock('settle'), 'setup/settle');
  }
}

export async function platformInput(observer, action, before, record = {}) {
  if (!before.outside || !before.outsideAfter || digest(before.outside) !== digest(before.outsideAfter))
    throw new Error('outside observation changed or was missing across the checkpoint; platform input refused');
  const observation = {source: 'platform/accessibility', identity: observer.identity, start: Date.now()};
  record.inputObservation = observation;
  try {
    const outside = await observer.tree();
    observation.result = outside;
    observation.end = Date.now();
    if (!outside || outside.error || outside.errors?.length || outside.truncated || outside.frontmost === false)
      throw new Error('fresh outside observation is incomplete or inactive; platform input refused');
    if (digest(outside) !== digest(before.outsideAfter))
      throw new Error('outside observation changed after the checkpoint; platform input refused');
    const aim = targetPoint(outside, action.target);
    const input = delivered(await observer.input(action, aim.point, outside), 'platform/input');
    return {aim, input, inputObservation: observation};
  } catch (error) {
    observation.end ??= Date.now();
    observation.error = error.message;
    throw error;
  }
}

export function inputOutcome(checkpoint) {
  const state = modelState(checkpoint?.state);
  delete state.focus;
  return {model: modelElements(checkpoint?.model ?? {}), state,
    outside: externalElements(checkpoint?.outside ?? {}).map(e =>
      Object.fromEntries(['identifier', 'label', 'type', 'enabled', 'selected', 'value'].filter(k => k in e).map(k => [k, e[k]])))};
}

export async function run(options) {
  const directory = resolve(options.out);
  mkdirSync(dirname(directory), {recursive: true});
  mkdirSync(directory, {recursive: false});
  const report = {options, started: Date.now(), sessions: [], findings: [],
    limitation: 'Outside snapshots can activate accessibility machinery. These timings are instrument costs, not app performance.'};
  try {
    if (process.platform !== 'darwin') throw new Error('see requires macOS');
    if (options.host === 'ios') {
      const devices = JSON.parse(command('xcrun', ['simctl', 'list', 'devices', '--json']));
      const found = Object.values(devices.devices).flat().find(d => d.udid === options.sim && d.state === 'Booted');
      if (!found) throw new Error('--sim must name a booted simulator by UDID; see never boots one');
      process.env.EXACT_SIM = options.sim;
      process.env.EXACT_SHOW_SIMULATOR = '0';
    }
    const helper = options.host === 'macos' ? macHelper() : null;
    const {open} = await import('../agent-session.mjs');
    for (const leg of options.action ? ['driver', 'platform'] : ['observe']) {
      let session, pid, observer;
      const result = {leg, findings: []};
      report.sessions.push(result);
      try {
        session = await open({host: options.host, app: options.app, chrome: 'platform', timing: 'agent',
          env: {EXACT_AGENT_ACTIVATE: '1'},
          touch: options.host === 'ios' ? 'drag' : 'agent', seed: 1172, epoch: '2026-01-01T00:00:00Z',
          onProcess: child => { if (options.host === 'macos') pid = child.pid; }});
        observer = platform({host: options.host, pid, session, helper});
        observer.activate();
        result.launch = session.launchFacts;
        await setupInput(session, options.setup);
        const before = await checkpoint(session, observer, resolve(directory, leg, 'before'));
        result.before = before;
        if (options.action) {
          const start = Date.now();
          try {
            if (leg === 'driver') result.input = await internalInput(session, options.action);
            else Object.assign(result, await platformInput(observer, options.action, before, result));
          } catch (error) { result.findings.push(blind(`${leg}/input`, error.message)); }
          result.inputMs = Date.now() - start;
          result.after = await checkpoint(session, observer, resolve(directory, leg, 'after'));
        }
      } catch (error) { result.findings.push(blind(leg, error.message)); }
      finally {
        try { await session?.close(); } catch (error) { result.findings.push(blind(`${leg}/cleanup`, error.message)); }
      }
      report.findings.push(...result.findings);
      for (const phase of ['before', 'after']) for (const finding of result[phase]?.report.findings ?? [])
        report.findings.push({leg, phase, ...finding});
    }
    if (options.action) {
      const [a, b] = report.sessions;
      if (!a.before || !b.before || digest(inputOutcome(a.before)) !== digest(inputOutcome(b.before)))
        report.findings.push(blind('input-comparison', 'initial conditions differ or were not observed'));
      else {
        const incomplete = !a.after || !b.after || [a, b].some(s => s.findings.length || ['before', 'after'].some(k => s[k].report.status === 'inconclusive'));
        if (incomplete) report.findings.push(blind('input-comparison', 'delivery or a checkpoint was inconclusive'));
        if (a.after && b.after && digest(inputOutcome(a.after)) !== digest(inputOutcome(b.after)))
          report.findings.push({kind: 'input-outcome', provisional: incomplete, reason: 'driver and platform input produced different sampled outcomes'});
      }
    }
  } catch (error) { report.findings.push(blind('see', error.message)); }
  report.ended = Date.now();
  report.status = verdict(report.findings);
  json(resolve(directory, 'report.json'), report);
  const counts = {};
  for (const finding of report.findings) counts[finding.kind] = (counts[finding.kind] ?? 0) + 1;
  console.log(JSON.stringify({status: report.status, milliseconds: report.ended - report.started, findings: counts, report: resolve(directory, 'report.json')}, null, 2));
  return {agreement: 0, disagreement: 1, inconclusive: 2}[report.status];
}

export async function main(argv) {
  try {
    const options = parse(argv);
    if (options.help) {
      console.log('see macos|ios --app <name> --out <new-directory> [--sim <owned-UDID>]\n  [--setup-tap <test-id>] [--tap <test-id> | --drag <test-id> <dx> <dy> <ms>]\nSee scripts/see/README.md for checkpoint semantics, blind spots and removal.');
      return 0;
    }
    return await run(options);
  } catch (error) { console.error(error.message); return 2; }
}
if (import.meta.main) process.exitCode = await main(process.argv.slice(2));
