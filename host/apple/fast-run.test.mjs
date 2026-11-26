import { test, expect } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { canFastSimulatorRun, plistAllowsLocalNetworking, stagePlanInSimulatorContainer, tryFastSimulatorRun } from './fast-run.mjs';

test('canFastSimulatorRun accepts only development simulator --ios --run invocations', () => {
  const base = { ios: true, device: false, tv: false, ipa: null, unsigned: false, production: false, distribution: false, gallery: false };
  expect(canFastSimulatorRun(['--ios', '--run'], base)).toBe(true);
  expect(canFastSimulatorRun(['--ios', '--run', '--rebuild'], base)).toBe(false);
  expect(canFastSimulatorRun(['--ios', '--run', '--json'], base)).toBe(false);
  expect(canFastSimulatorRun(['--ios'], base)).toBe(false);
  expect(canFastSimulatorRun(['--ios', '--run'], { ...base, production: true })).toBe(false);
  expect(canFastSimulatorRun(['--ios', '--run'], { ...base, device: true })).toBe(false);
});

test('plistAllowsLocalNetworking detects NSAllowsLocalNetworking and NSAllowsArbitraryLoads', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-fast-plist-'));
  try {
    writeFileSync(resolve(dir, 'Info.plist'), '<plist><dict></dict></plist>');
    expect(plistAllowsLocalNetworking(dir)).toBe(false);
    writeFileSync(resolve(dir, 'Info.plist'), '<plist><dict><key>NSAllowsLocalNetworking</key><true/></dict></plist>');
    expect(plistAllowsLocalNetworking(dir)).toBe(true);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('stagePlanInSimulatorContainer copies plan and module pair into simulator data container', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-fast-stage-'));
  const override = resolve(dir, 'override'), data = resolve(dir, 'sim-data');
  try {
    mkdirSync(override, { recursive: true });
    mkdirSync(data, { recursive: true });
    writeFileSync(resolve(override, 'app.plan'), 'PLAN');
    writeFileSync(resolve(override, 'app.module.json'), '{}');
    writeFileSync(resolve(override, 'app.hbc'), 'HBC');
    const staged = stagePlanInSimulatorContainer('SIM-1', 'com.example.app', resolve(override, 'app.plan'), {
      containerFn: () => data,
    });
    expect(staged).toBe(resolve(data, 'tmp/exact-plan/app.plan'));
    expect(readFileSync(staged, 'utf8')).toBe('PLAN');
    expect(existsSync(resolve(data, 'tmp/exact-plan/app.hbc'))).toBe(true);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('tryFastSimulatorRun handles live reuse, checks moduleRefusal on TS edits, and stages standalone overrides', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-fast-run-'));
  const receiptFile = resolve(dir, 'receipt.json');
  const builtBundle = resolve(dir, 'App.app');
  mkdirSync(builtBundle, { recursive: true });
  writeFileSync(receiptFile, JSON.stringify({ build: { trust: 'development' } }));
  const app = { id: 'com.example.app', name: 'app', displayName: 'App', dir, manifest: {} };
  const dev = { udid: 'SIM-1', name: 'iPhone 17 Pro' };
  const fakeAgentLaunch = {
    refuseBackendOrigin: () => {},
    simulatorApps: async (_u, _ids, fn) => await fn(),
  };
  try {
    // 1. Live dev loop with Contract-only change skips bake and launches with EXACT_DEV_PLAN.
    const launches = [];
    const liveRes = await tryFastSimulatorRun({
      app, dev, args: ['--ios', '--run'],
      launchEnv: { EXACT_DEV_PLAN: 'http://127.0.0.1:8765/' },
      install: () => { throw new Error('should skip install when installedAlready'); },
      installedCheck: () => true,
      bootSim: () => dev,
      launchFn: async (_d, _id, env) => { launches.push(env); return { pid: 10 }; },
      agentLaunchModule: fakeAgentLaunch,
      agentBuildModule: {
        nativeBuild: () => ({ built: builtBundle, receipt: receiptFile, changed: ['app.contract'], missing: false }),
        planSource: (p) => p.endsWith('.contract') || p.endsWith('.ts'),
        buildRefusal: () => null,
        makeOverride: async () => { throw new Error('contract-only live reload should not need bake'); },
      },
      log: () => {},
    });
    expect(liveRes.handled).toBe(true);
    expect(launches[0]).toEqual({ SIMCTL_CHILD_EXACT_DEV_PLAN: 'http://127.0.0.1:8765/' });

    // 2. Live dev loop with app.ts change checks makeOverride/moduleRefusal and falls back on refusal.
    const tsRefused = await tryFastSimulatorRun({
      app, dev, args: ['--ios', '--run'],
      launchEnv: { EXACT_DEV_PLAN: 'http://127.0.0.1:8765/' },
      install: () => {}, installedCheck: () => true, bootSim: () => dev,
      agentLaunchModule: fakeAgentLaunch,
      agentBuildModule: {
        nativeBuild: () => ({ built: builtBundle, receipt: receiptFile, changed: ['app.ts'], missing: false }),
        planSource: () => true,
        buildRefusal: () => null,
        makeOverride: async () => ({ refusal: 'app.ts changed its grants' }),
      },
      log: () => {},
    });
    expect(tsRefused).toEqual({ handled: false, reason: 'app.ts changed its grants' });

    // 3. Standalone plan-only edit stages override into simulator container and passes SIMCTL_CHILD_EXACT_PLAN.
    launches.length = 0;
    const standalone = await tryFastSimulatorRun({
      app, dev, args: ['--ios', '--run'],
      launchEnv: {},
      install: () => {}, installedCheck: () => true, bootSim: () => dev,
      stagePlan: () => '/sim-data/tmp/exact-plan/app.plan',
      launchFn: async (_d, _id, env) => { launches.push(env); return { pid: 11 }; },
      agentLaunchModule: fakeAgentLaunch,
      agentBuildModule: {
        nativeBuild: () => ({ built: builtBundle, receipt: receiptFile, changed: ['app.contract'], missing: false }),
        planSource: () => true,
        buildRefusal: () => null,
        makeOverride: async () => ({ made: '/target/plan/app.plan' }),
      },
      log: () => {},
    });
    expect(standalone.handled).toBe(true);
    expect(launches[0]).toEqual({ SIMCTL_CHILD_EXACT_PLAN: '/sim-data/tmp/exact-plan/app.plan' });

    // 4. Widget contract change bypasses fast reuse so PlugIns/ExactWidgets.appex/plans/*.plan refreshes (LLP 1156.008.000 D6.8).
    const widgetApp = { ...app, manifest: { widgets: [{ kind: 'card', source: 'widget.contract' }] } };
    const widgetChange = await tryFastSimulatorRun({
      app: widgetApp, dev, args: ['--ios', '--run'],
      launchEnv: {},
      install: () => {}, installedCheck: () => true, bootSim: () => dev,
      agentLaunchModule: fakeAgentLaunch,
      agentBuildModule: {
        nativeBuild: () => ({ built: builtBundle, receipt: receiptFile, changed: ['widget.contract'], missing: false }),
        planSource: () => true,
        buildRefusal: () => null,
      },
      log: () => {},
    });
    expect(widgetChange).toEqual({ handled: false, reason: 'widget contracts changed' });
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
