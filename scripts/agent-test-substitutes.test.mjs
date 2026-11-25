// A sponsor store reads membership once when it opens and its purchase outcome at each call.
import { expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { runTests } from './agent-test.mjs';
import { engineTests } from './coverage.mjs';
import { contractTool } from './tools.mjs';

const engines = await engineTests(['chromium']);

engines.chromium('purchase outcomes are live, carried into a restart, and reset on replay and between files', async () => {
  const app = mkdtempSync(join(tmpdir(), 'exact-sponsor-'));
  try {
    const pkg = join(app, 'node_modules', '@fixture', 'store'), file = join(app, 'app.test.contract');
    mkdirSync(join(pkg, 'web'), {recursive: true});
    const json = (path, value) => writeFileSync(path, JSON.stringify(value));
    json(join(app, 'package.json'), { dependencies: { '@fixture/store': '1.0.0' } });
    json(join(app, 'app.json'), { name:'Sponsor', app:{ id:'test.sponsor', name:'Sponsor' }, host:{ web:{} }, rust:false });
    json(join(pkg, 'package.json'), { name:'@fixture/store', version:'1.0.0', exports:{ '.':'./exact.json', './web':'./web/exact.json' } });
    const receipts = join(app, 'node_modules', '@fixture', 'receipts');
    mkdirSync(receipts);
    json(join(receipts, 'package.json'), {name:'@fixture/receipts', version:'1.0.0', exports:{'.':'./exact.json'}});
    json(join(receipts, 'exact.json'), {kind:'interface', name:'@fixture/receipts', version:'1.0.0', shapes:{Receipt:{count:'number'}}, names:[]});
    json(join(pkg, 'exact.json'), {
      kind: 'interface', name: '@fixture/store', version: '1.0.0', substitute: 'implementation',
      shapeImports: { '@fixture/receipts': ['Receipt'] },
      agent: { active: 'bool', outcome: 'string', frame: 'DOMRect', receipts: 'Receipt[]', document: 'json<Membership>', payload: 'json<any>' },
      shapes: { Membership: { active:'bool' }, Result: { outcome:'string' } },
      names: [
        { source:'membership', answers:'Membership', substitute:{ default:{ active:false } } },
        { send:'purchase', answers:'Result' },
      ],
    });
    json(join(pkg, 'web', 'exact.json'), {
      kind:'implementation', implements:'@fixture/store@^1.0', platform:'web', targets:['web'],
      names:[], sources:{javascript:['index.js']}, artifacts:[],
    });
    writeFileSync(join(pkg, 'web', 'index.js'), `export function implementation(ctx) {
      let active, outcome;
      return {
        async preferred(values) {
          if (values.outcome === 'clocked' && outcome !== 'clocked') await new Promise(resolve => ctx.clocked(resolve));
          else await Promise.resolve();
          active ??= values.active ?? false;
          outcome = values.outcome ?? 'idle';
          if (outcome === 'navigate' && !sessionStorage.navigated) {
            sessionStorage.navigated = '1';
            location.reload();
          }
        },
        membership() { return {active}; },
        purchase() { return {outcome}; }
      };
    }`);
    writeFileSync(join(app, 'app.contract'), `use purchase, membership, Membership, Result from "@fixture/store"
component Store
  resource sponsor = membership() as shape Membership else empty()
  mutation bought as shape Result
  action buy
    send bought = purchase()
  view
    column
      button "Buy" testId="buy" press=buy
      text (sponsor.active ? "active" : "inactive") testId="sponsor"
      match bought
        case some(r)
          text r.outcome testId="result"
        case none
          text "waiting"
`);
    const compiler = contractTool({targetDir: process.env.CARGO_TARGET_DIR});
    writeFileSync(file, `test "bad substitutes"
  prefer extension absent.active=clear
  prefer extension store.absent=clear
  prefer extension store.active="yes"
  prefer extension store.membership={"active":"yes"}
  clock settle
  prefer extension store.active="clear"
  prefer extension store.receipts=[17]
  prefer extension store.document="not an object"
  prefer extension store.receipts=[{"count":false}]
  prefer extension store.frame={"x":0}
`);
    const checked = spawnSync(compiler.cmd, [...compiler.pre, 'test', file], {env: compiler.env, encoding:'utf8'});
    expect(checked.status).toBe(1);
    for (const [line, reason] of [[2,'unknown package `absent`'], [3,'unknown substitute `absent`'], [4,'expects bool'], [5,'store.membership.active'], [7,'expects bool'], [8,'expects Receipt[]'], [9,'expects json<Membership>'], [10,'expects Receipt[]'], [11,'expects DOMRect']]) {
      expect(checked.stderr.split('\n').find(s => s.includes(`:${line}:`)), checked.stderr).toContain(reason);
    }
    expect(checked.stderr.split('\n').find(s => s.includes(':5:'))).toContain('web, macos, ios, tvos, linux, windows, android');
    writeFileSync(file, `prefer extension store.active=17
test "overrides the file"
  prefer extension store.active=true
  clock settle
`);
    const shadowed = spawnSync(compiler.cmd, [...compiler.pre, 'test', file], {env: compiler.env, encoding:'utf8'});
    expect(shadowed.stderr).toContain(':1:');
    expect(shadowed.stderr).toContain('expects bool');
    expect(shadowed.status).toBe(1);
    const nested = join(app, 'nested');
    mkdirSync(nested);
    symlinkSync(join(app, 'app.contract'), join(nested, 'app.contract'));
    writeFileSync(join(nested, 'app.test.contract'), 'test "ancestor dependency"\n  prefer extension store.receipts=[{"count":1}]\n');
    const relative = spawnSync(compiler.cmd, [...compiler.pre, 'test', 'app.test.contract'], {cwd:nested, env:compiler.env, encoding:'utf8'});
    expect(relative.status, relative.stderr).toBe(0);
    const buy = outcome => `  tap "buy"
  clock settle
  expect text "result" == "${outcome}"
`;
    writeFileSync(file, `test "buy"
  prefer extension store.document={"active":true}
  prefer extension store.payload=[1,null,"ok"]
  expect state sponsor.active == false
${buy('idle')}${['success', 'cancel', 'failure'].map(outcome => `  prefer extension store.outcome="${outcome}"\n${buy(outcome)}`).join('')}  prefer extension store.active=true
  expect state sponsor.active == false
  reload
  expect state sponsor.active == true
${buy('failure')}  prefer extension store.outcome="clear"
${buy('clear')}  prefer extension store.active=clear
  reload
  expect state sponsor.active == false
  prefer extension store.active=true
  expect state sponsor.active == false
`);
    const webDist = join(app, 'dist');
    const build = spawnSync(process.execPath, ['host/web/build.mjs', '--render', 'none'], {
      cwd: resolve(import.meta.dir, '..'), env: {...process.env, EXACT_APP_DIR:app, EXACT_WEB_DIST:webDist}, encoding:'utf8',
    });
    expect(build.status, build.stdout + build.stderr).toBe(0);
    const drive = async (file, replayLaunch) => {
      const r = await runTests({host:'web', app, webDist, file, replayLaunch, atEnd: async (s, t) => {
        if (t.name === 'buy') {
          // A new document can also open without an authored reload supplying a fresh table.
          await s.carrier.reset({keep:true});
          await s.clock('settle');
          expect((await s.find('sponsor')).props.text).toBe('active');
        }
        return t.launch;
      }});
      expect(r.results.map(t => t.failures)).toEqual(r.results.map(() => []));
      return r.results[0].end;
    };
    const end = await drive(file);
    expect(end.substitutes).toEqual({'store.active':true, 'store.outcome':'clear', 'store.document':{active:true}, 'store.payload':[1,null,'ok']});
    await drive(file, end);
    const next = join(app, 'membership.test.contract');
    writeFileSync(next, `prefer extension store.outcome="success"
test "another file starts with default membership"
  expect state sponsor.active == false
${buy('success')}test "a test overrides its file"
  prefer extension store.outcome=clear
  prefer extension store.active=true
  expect state sponsor.active == true
${buy('idle')}test "the package reloads on receipt"
  expect state sponsor.active == false
  prefer extension store.active=true
  prefer extension store.outcome="navigate"
  clock settle
  expect state sponsor.active == true
${buy('navigate')}`);
    await drive(next);
    writeFileSync(next, `test "the package waits for the next clock step"
  expect state sponsor.active == false
  prefer extension store.outcome="clocked"
  clock +1
${buy('clocked')}`);
    await drive(next);
  } finally {
    rmSync(app, {recursive:true, force:true});
  }
}, 180000);

engines.chromium('a preference settles without paint, supplies every permission field, and reports a refusal at its step', async () => {
  const app = mkdtempSync(join(tmpdir(), 'exact-permission-substitutes-'));
  try {
    const pkg = join(app, 'node_modules', '@fixture', 'chime');
    mkdirSync(pkg, {recursive: true});
    mkdirSync(join(app, 'node_modules', '@exact'));
    symlinkSync(resolve(import.meta.dir, '../packages/nfc'), join(app, 'node_modules', '@exact', 'nfc'));
    const json = (path, value) => writeFileSync(path, JSON.stringify(value));
    json(join(app, 'package.json'), {dependencies: {'@fixture/chime':'1.0.0', '@exact/nfc':'1.0.0'}});
    json(join(app, 'app.json'), {name:'Permission', app:{id:'test.permission', name:'Permission'}, host:{web:{}}, rust:false});
    json(join(pkg, 'package.json'), {name:'@fixture/chime', version:'1.0.0', exports:{'.':'./exact.json'}});
    const state = 'enum<granted|denied|prompt>';
    json(join(pkg, 'exact.json'), {
      kind:'interface', name:'@fixture/chime', version:'1.0.0',
      permission: ['state','access.level'].map((field, i) => ({
        name: i ? 'ping' : 'chime', grants:[], status:'chimeStatus', field, denied:'NotAllowedError'
      })),
      shapes:{ ChimeStatus:{available:'bool', state, access:'Access'}, Access:{level:state, audible:'bool'} },
      names:[{source:'chimeStatus', live:true, answers:'ChimeStatus',
        substitute:{default:{available:true, state:'prompt', access:{level:'prompt', audible:true}}}}],
    });
    writeFileSync(join(app, 'app.contract'), `use chimeStatus, ChimeStatus from "@fixture/chime"
use nfcStatus, NFCStatus from "@exact/nfc"
component App
  resource c = chimeStatus() as shape ChimeStatus else empty()
  resource n = nfcStatus() as shape NFCStatus else empty()
  view
    column
      button "Ready" testId="ready"
      text n.state
      text c.state
`);
    const file = join(app, 'app.test.contract'), webDist = join(app, 'dist');
    writeFileSync(file, `test "bad records"
  prefer extension chime.chimeStatus={"access":{"audible":true}}
  clock settle
  prefer extension chime.chimeStatus={"available":true,"access":{"level":"denied","audible":true}}
  prefer extension chime.chimeStatus={"available":true,"access":{}}
`);
    const compiler = contractTool({targetDir:process.env.CARGO_TARGET_DIR});
    const checked = spawnSync(compiler.cmd, [...compiler.pre, 'test', file], {env:compiler.env, encoding:'utf8'});
    for (const [line, field] of [[2,'available'], [4,'prefer permission.ping'], [5,'audible']]) {
      expect(checked.stderr.split('\n').find(s => s.includes(`:${line}:`)), checked.stderr).toContain(field);
    }
    writeFileSync(file, `prefer extension nfc.field="contextTag"
prefer extension nfc.tags=[{"id":"contextTag","records":[]}]
test "missing tag at launch"
  prefer extension nfc.tags=[]
  clock settle
`);
    const launch = spawnSync(compiler.cmd, [...compiler.pre, 'test', file], {env:compiler.env, encoding:'utf8'});
    expect(launch.stderr).toMatch(/:1:.*no tag `contextTag` in `nfc.tags`/);
    const build = spawnSync(process.execPath, ['host/web/build.mjs', '--render', 'none'], {
      cwd: resolve(import.meta.dir, '..'), env:{...process.env, EXACT_APP_DIR:app, EXACT_WEB_DIST:webDist}, encoding:'utf8',
    });
    expect(build.status, build.stdout + build.stderr).toBe(0);
    writeFileSync(file, `test "permission fields"
  prefer permission.ping denied
  tap "ready"
  prefer extension chime.chimeStatus={"available":false,"access":{"audible":true}}
  clock settle
  expect state c.available == false
  expect state c.state == "prompt"
  expect state c.access.level == "denied"

test "a missing NFC tag"
  prefer extension nfc.tags=[]
  clock settle
  prefer extension nfc.field="contextTag"
`);
    let carrier, deadline, timedOut = false, resumed;
    const resume = () => carrier.call('Emulation.setVirtualTimePolicy', {policy:'advance'});
    try {
      const {results} = await runTests({host:'web', app, webDist, file,
        async atStep(s, step) {
          if (step.op !== 'tap') return;
          carrier = s.carrier;
          await carrier.call('Emulation.setVirtualTimePolicy', {policy:'pause'});
          deadline = setTimeout(() => { timedOut = true; resumed = resume(); }, 5000);
        },
        atLine(step) {
          if (step.line === 5 && !resumed) { clearTimeout(deadline); resumed = resume(); }
        },
      });
      expect(results[0].failures).toEqual([]);
      expect(timedOut).toBe(false);
      expect(results[1].failures).toEqual(['a missing NFC tag: line 13: prefer: prefer extension: no tag `contextTag` in `nfc.tags`']);
    } finally { clearTimeout(deadline); await resumed; }
  } finally { rmSync(app, {recursive:true, force:true}); }
}, 180000);
