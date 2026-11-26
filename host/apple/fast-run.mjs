// Fast development simulator reuse for `ios --run` (LLP 1189 D3).
// Reuses an existing fresh or plan-only-stale simulator bundle without invoking Cargo or Swift,
// validating ATS local-networking, backend origin, and module compatibility (grants/surfaces/ABI).
import { existsSync, readFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { appleExecutableName } from '../../scripts/app.mjs';
import { parseLoopbackDevURL } from '../web/dev-discovery.mjs';
import { simulatorPlanPath } from './assets.mjs';
import { installedAlready, simulator, simulatorAppContainer } from './devices.mjs';
import { EXTENSION_KINDS } from './extensions.mjs';
import { launchAndOptionallyStream } from './sim-logs.mjs';

/** Whether this invocation is eligible for fast simulator reuse. */
export function canFastSimulatorRun(args, {
  ios, device, tv, ipa, unsigned, production, distribution, packaged, gallery,
} = {}) {
  if (!ios || device || tv || ipa || unsigned || production || distribution || gallery) return false;
  if (!args.includes('--run')) return false;
  for (const flag of ['--rebuild', '--host', '--embed', '--json', '--no-install', '--prebuild', '--link', '--watchos', '--painted-benchmark', '--test']) {
    if (args.includes(flag)) return false;
  }
  if (packaged && packaged.size < EXTENSION_KINDS.length) return false;
  return true;
}

/** Whether the built `.app`'s `Info.plist` permits local HTTP networking for live reload. */
export function plistAllowsLocalNetworking(bundlePath, { readPlist = null } = {}) {
  const plistPath = resolve(bundlePath, 'Info.plist');
  if (!existsSync(plistPath)) return false;
  if (readPlist) return Boolean(readPlist(plistPath));
  let text = '';
  try {
    const raw = readFileSync(plistPath);
    if (raw.subarray(0, 6).toString('ascii') === 'bplist') {
      const r = spawnSync('plutil', ['-convert', 'json', '-o', '-', plistPath], { encoding: 'utf8' });
      if (r.status !== 0) return false;
      const parsed = JSON.parse(r.stdout || '{}');
      const ats = parsed?.NSAppTransportSecurity;
      return Boolean(ats?.NSAllowsLocalNetworking || ats?.NSAllowsArbitraryLoads);
    }
    text = raw.toString('utf8');
  } catch { return false; }
  return /<key>NSAllows(?:LocalNetworking|ArbitraryLoads)<\/key>\s*<true\s*\/>/.test(text);
}

/** Stage a baked plan override inside the app's simulator data container so macOS TCC never blocks reads. */
export function stagePlanInSimulatorContainer(udid, appId, planFile, {
  containerFn = simulatorAppContainer,
  stageFn = simulatorPlanPath,
} = {}) {
  const dataRoot = containerFn(udid, appId, 'data');
  const stageDir = resolve(dataRoot, 'tmp/exact-plan');
  rmSync(stageDir, { recursive: true, force: true });
  return stageFn(planFile, stageDir);
}

/** Attempt fast simulator launch under the held Apple build lock.
 * Returns `{ handled: true }` if launched, or `{ handled: false, reason }` to fall back to a full build. */
export async function tryFastSimulatorRun({
  app,
  dev,
  args,
  launchEnv = {},
  install,
  agentBuildModule = null,
  agentLaunchModule = null,
  plistCheck = plistAllowsLocalNetworking,
  installedCheck = installedAlready,
  bootSim = simulator,
  launchFn = launchAndOptionallyStream,
  stagePlan = stagePlanInSimulatorContainer,
  log = console.log,
} = {}) {
  const agentBuild = agentBuildModule ?? await import('../../scripts/agent-build.mjs');
  const agentLaunch = agentLaunchModule ?? await import('../../scripts/agent-launch.mjs');
  const state = agentBuild.nativeBuild('ios', app);
  if (state.missing || !state.receipt) return { handled: false, reason: 'no existing simulator build' };
  let receipt;
  try { receipt = JSON.parse(readFileSync(state.receipt, 'utf8')); } catch { return { handled: false, reason: 'unreadable receipt' }; }
  if (receipt.build?.trust !== 'development') return { handled: false, reason: 'existing build is not development trust' };
  if (receipt.build?.widgetGallery) return { handled: false, reason: 'existing build is a widget gallery build' };
  try { agentLaunch.refuseBackendOrigin(state, app, 'ios'); } catch { return { handled: false, reason: 'backend origin changed' }; }

  const liveUrl = launchEnv.EXACT_DEV_PLAN ?? null;
  const fresh = state.changed.length === 0;
  const hasWidgetEntries = Boolean(app.manifest?.widgets?.length || app.manifest?.activities?.length);
  if (hasWidgetEntries && state.changed.some((path) => /\.contract$/.test(path))) {
    return { handled: false, reason: 'widget contracts changed' };
  }
  const planOnly = !fresh && state.changed.every((path) => agentBuild.planSource(path, app));
  if (!fresh && !planOnly) return { handled: false, reason: 'native sources changed' };

  if (liveUrl && liveUrl.startsWith('http://') && !parseLoopbackDevURL(liveUrl) && !plistCheck(state.built)) {
    return { handled: false, reason: 'existing bundle lacks NSAllowsLocalNetworking for non-loopback http live reload' };
  }

  let stagedOverride = null;
  if (planOnly) {
    if (agentBuild.buildRefusal(app, receipt)) return { handled: false, reason: 'build cannot take a plan override' };
    // Always bake & check moduleRefusal when TypeScript changed (or standalone) so changed grants/surfaces trigger a rebuild.
    const needsBake = !liveUrl || state.changed.some((p) => /\.tsx?$/.test(p));
    if (needsBake) {
      const override = await agentBuild.makeOverride('ios', app, state, {
        log: resolve(app.dir, '.exact/ios-plan-bake.log'),
      });
      if (override.refusal) return { handled: false, reason: override.refusal };
      if (override.failed) throw new Error(override.failed);
      if (!liveUrl) stagedOverride = override.made;
    }
  }

  const booted = bootSim(dev.udid, { app: app.id, dir: app.dir, hint: '--sim <udid|name> (or EXACT_SIM)' });
  const executable = appleExecutableName(app, true);
  const childEnv = { ...(liveUrl ? { SIMCTL_CHILD_EXACT_DEV_PLAN: liveUrl } : {}) };
  const logs = args.includes('--logs');

  await agentLaunch.simulatorApps(booted.udid, [app.id], async () => {
    if (!installedCheck(booted.udid, app.id, state.built, executable)) {
      install(booted, state.built, app, false, false, executable);
    }
    if (stagedOverride) {
      const stagedPath = stagePlan(booted.udid, app.id, stagedOverride);
      childEnv.SIMCTL_CHILD_EXACT_PLAN = stagedPath;
    }
    const modeNote = liveUrl
      ? `live reload via ${liveUrl}`
      : stagedOverride
        ? 'baked plan override; launching from the Home Screen runs the installed plan'
        : 'unchanged';
    log(`host/apple: reusing fresh simulator build on ${booted.name} (${modeNote}; --rebuild to force full build)`);
    await launchFn(booted, app.id, childEnv, { logs, executable });
  }, { name: booted.name });

  return { handled: true };
}
