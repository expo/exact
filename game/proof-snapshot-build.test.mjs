import {test, expect} from 'bun:test';
import {spawn} from 'node:child_process';
import {cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, utimesSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {createGame} from './new.mjs';

// One scaffold and its shared Cargo target exercise the whole snapshot/build boundary.
test('a proof leaves ordinary builds usable and repins changed logic and added art', async () => {
  const parent = realpathSync(mkdtempSync(resolve(tmpdir(), 'proof-build-'))), app = resolve(parent, 'tidal-proof');
  const env = {...process.env, EXACT_UPDATE_TRUST:'development'};
  for (const name of ['EXACT_APP_DIR','CARGO_TARGET_DIR','CARGO_BUILD_BUILD_DIR','EXACT_PROOF_BUILD_OUT','EXACT_PROOF_OUT','EXACT_WEB_DIST']) delete env[name];
  const run = args => new Promise((ok, fail) => {
    const child = spawn(process.execPath, args, {cwd:app, env, stdio:['ignore','pipe','pipe']});
    let log = '';
    for (const stream of [child.stdout,child.stderr]) stream.on('data', bytes => {log += bytes; process.stdout.write(bytes);});
    child.on('error', fail);
    child.on('close', code => code === 0 ? ok(log) : fail(new Error(`exit ${code}: ${args.join(' ')}\n${log.slice(-8000)}`)));
  });
  const prove = async args => {
    const log = await run(['exact.mjs','prove',...args]);
    const root = log.match(/^ARTIFACTS (.+)$/m)[1];
    return {root, pins:JSON.parse(readFileSync(resolve(app,'pins.json'),'utf8'))};
  };
  let passed = false;
  try {
    createGame(app, undefined, {assets:true});
    // Keep the regression about build identity; the starter's gameplay has its own proof.
    writeFileSync(resolve(app,'proof.mjs'), `import {resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {proof} from ${JSON.stringify(resolve(import.meta.dir,'proof.mjs'))};
if (import.meta.main) await proof(import.meta, async ({open,pin,pinSave,out}) => {
  const session = await open(); await session.tap('play');
  const world = session.world('world'); pin(0, await world.snapshot());
  const save = resolve(out,'start.world'); await world.save(save); pinSave('start',save);
  await session.close();
});\n`);
    const first = await prove([]);
    await run(['exact.mjs','web-build']);
    const source = resolve(app,'logic/src/lib.rs');
    const original = readFileSync(source,'utf8');
    expect(original).toContain('Transform::at(-4.0, 0.5, -3.0)');
    writeFileSync(source, original.replace('Transform::at(-4.0, 0.5, -3.0)', 'Transform::at(-5.0, 0.5, -3.0)'));
    const changed = await prove(['--repin']);
    expect(changed.pins.ticks['0']).not.toBe(first.pins.ticks['0']);
    expect(readFileSync(resolve(changed.root,'linux-0-1/run.log'),'utf8')).toContain('BUILD gpu stale');
    cpSync(resolve(app,'art/crate.gltf'),resolve(app,'art/second.gltf'));
    const added = await prove(['--repin']);
    expect(Object.keys(JSON.parse(readFileSync(resolve(added.root,'src/.baked-assets.json'),'utf8')))).toContain('second.model');
    await run(['exact.mjs','web-build']);
    passed = true;
  } finally {
    if (passed) rmSync(parent,{recursive:true,force:true});
    else console.error(`Retained failed snapshot proof: ${parent}`);
  }
}, 1800000);

test.skipIf(process.platform !== 'darwin')('macOS-first game receipts qualify generated inputs and invalidate edits', async () => {
  const {createHash} = await import('node:crypto');
  const root = resolve(import.meta.dir, '..');
  mkdirSync(resolve(root, 'target'), {recursive:true});
  const parent = mkdtempSync(resolve(root, 'target/game-receipt-')), app = resolve(parent, 'receipt-game');
  const env = {...process.env, EXACT_UPDATE_TRUST:'development', CARGO_NET_OFFLINE:'true'};
  for (const name of ['EXACT_APP_DIR', 'CARGO_TARGET_DIR', 'CARGO_BUILD_BUILD_DIR', 'EXACT_PROOF_BUILD_OUT', 'EXACT_PROOF_OUT', 'EXACT_WEB_DIST', 'EXACT_PROOF_REPIN']) delete env[name];
  let passed = false;
  try {
    createGame(app, undefined, {assets:true});
    const build = async (label, expected = 0) => {
      const log = resolve(parent, `${label}.log`), errors = resolve(parent, `${label}.build.log`);
      const child = Bun.spawn([process.execPath, resolve(root, 'host/apple/build.mjs'), app], {
        cwd:root, env:{...env, EXACT_APP_DIR:app}, stdout:Bun.file(log), stderr:Bun.file(errors),
      });
      const code = await child.exited, text = readFileSync(log, 'utf8') + readFileSync(errors, 'utf8');
      expect(code, text).toBe(expected);
      return text;
    };
    // Resolving a proof prepares shells before Apple starts; a direct build must qualify them too.
    rmSync(resolve(app, '.shells'), {recursive:true});
    await build('cold-direct');
    const probe = `import {nativeBuild} from ${JSON.stringify(resolve(root, 'scripts/agent-build.mjs'))};
import {resolveApp} from ${JSON.stringify(resolve(root, 'scripts/app.mjs'))};
console.log(JSON.stringify(nativeBuild('macos', resolveApp(${JSON.stringify(app)}))));`;
    const state = () => {
      const result = Bun.spawnSync([process.execPath, '--eval', probe], {cwd:root, env});
      expect(result.exitCode, result.stderr.toString()).toBe(0);
      return JSON.parse(result.stdout.toString());
    };
    const receipt = () => JSON.parse(readFileSync(state().receipt, 'utf8'));
    expect(state().missing).not.toBe(true);
    expect(state().changed).toEqual([]);
    writeFileSync(resolve(app, 'pins.json'), '{"ticks":{},"saves":{}}');
    writeFileSync(resolve(app, 'proof.mjs'), `import {proof} from ${JSON.stringify(resolve(import.meta.dir, 'proof.mjs'))};
await proof(import.meta, async ({open, check, pin, pinSave, out}) => {
  const s = await open();
  await s.tap('play');
  const world = s.world('world'), state = await world.snapshot();
  check('world starts', state.tick === 0);
  pin(0, state);
  const save = out + '/start.world';
  await world.save(save); pinSave('start', save);
  await s.close();
});
`);
    const prove = async (label, collecting = false, expected = collecting ? 3 : 0) => {
      const out = resolve(parent, label);
      mkdirSync(out);
      const child = Bun.spawn([process.execPath, resolve(app, 'proof.mjs'), 'macos'], {
        cwd:root, env:{...env, EXACT_PROOF_REPIN:collecting ? '1' : '0', EXACT_PROOF_OUT:out},
        stdout:Bun.file(resolve(out, 'run.log')), stderr:Bun.file(resolve(out, 'build.log')),
      });
      expect(await child.exited, readFileSync(resolve(out, 'run.log'), 'utf8')).toBe(expected);
      const summary = JSON.parse(readFileSync(resolve(out, 'summary.json'), 'utf8'));
      expect(summary.failures).toEqual([]);
      return {summary, log:readFileSync(resolve(out, 'run.log'), 'utf8')};
    };
    const first = await prove('cold', false, 3);
    writeFileSync(resolve(app, 'pins.json'), JSON.stringify(first.summary.pins));
    const declarations = ['keys.json', 'surfaces.json'].map(file => resolve(app, '.shells', file));
    const digest = bytes => createHash('sha256').update(bytes).digest('hex');
    const products = receipt().build.products;
    expect(state().changed).toEqual([]);
    const shell = ['app.json', 'apple/build.rs', 'apple/src/lib.rs'].map(file => resolve(app, '.shells', file));
    const assets = Object.keys(JSON.parse(readFileSync(resolve(app, '.baked-assets.json'), 'utf8')))
      .map(file => resolve(app, 'assets', file));
    expect(assets).toContain(resolve(app, 'assets/crate.model'));
    for (const path of [...shell, ...declarations, ...assets]) {
      const original = readFileSync(path), product = products.find(p => p.path === path);
      expect(product?.sha256).toBe(digest(original));
      expect(product?.bytes).toBe(original.length);
      // The receipt binds bytes, including at old mtimes and after deletion.
      writeFileSync(path, '{}\n');
      utimesSync(path, new Date(0), new Date(0));
      expect(state().changed.map(p => resolve(root, p))).toContain(path);
      rmSync(path);
      expect(state().changed.map(p => resolve(root, p))).toContain(path);
      writeFileSync(path, original);
      expect(state().changed).toEqual([]);
    }
    expect((await prove('cached')).log).toContain('BUILD cached');
    expect((await prove('collecting', true)).log).toContain('BUILD cached');
    const [keys, surfaces] = declarations;
    rmSync(keys);
    expect(state().changed.map(p => resolve(root, p))).toContain(keys);
    await build('missing-keys');
    expect(existsSync(keys)).toBe(false);
    expect(receipt().build.products.some(p => p.path === keys)).toBe(false);
    expect(state().changed).toEqual([]);
    await build('missing-keys-again');
    expect(existsSync(keys)).toBe(false);
    expect(state().changed).toEqual([]);
    await prove('missing-keys-proof');
    const surfaceBytes = readFileSync(surfaces);
    rmSync(surfaces);
    expect(await build('missing-surfaces', 1)).toContain("build the game's GPU shell before its host");
    writeFileSync(surfaces, surfaceBytes);
    const logic = resolve(app, 'logic/src/lib.rs'), source = readFileSync(logic, 'utf8');
    expect(source).toContain('pub seed: u64,');
    expect(source).toContain('"KeyE"');
    writeFileSync(logic, source.replace('pub seed: u64,', 'pub seed: u64,\n    #[live]\n    pub bonus: f32,').replace('"KeyE"', '"KeyQ"'));
    expect(state().changed.length).toBeGreaterThan(0);
    const edited = await prove('edited', true);
    expect(edited.log).toContain('BUILD stale or missing receipt');
    expect(state().changed).toEqual([]);
    expect(readFileSync(declarations[0], 'utf8')).toContain('KeyQ');
    expect(readFileSync(declarations[1], 'utf8')).toContain('bonus');
    for (const path of declarations) {
      const product = receipt().build.products.find(p => p.path === path);
      expect(product.sha256).toBe(digest(readFileSync(path)));
      expect(product.sha256).not.toBe(products.find(p => p.path === path).sha256);
    }
    writeFileSync(resolve(app, 'pins.json'), JSON.stringify(edited.summary.pins));
    expect((await prove('edited-cached')).log).toContain('BUILD cached');
    const art = resolve(app, 'art/crate.gltf'), model = resolve(app, 'assets/crate.model');
    const gltf = JSON.parse(readFileSync(art, 'utf8')), previous = receipt().build.products.find(p => p.path === model);
    gltf.nodes[0].translation = [1, 0, 0];
    writeFileSync(art, JSON.stringify(gltf));
    expect(state().changed.map(p => resolve(root, p))).toContain(art);
    const rebaked = await prove('art-edited', true);
    expect(rebaked.log).toContain('BUILD stale or missing receipt');
    expect(state().changed).toEqual([]);
    const product = receipt().build.products.find(p => p.path === model);
    expect(product.sha256).toBe(digest(readFileSync(model)));
    expect(product.sha256).not.toBe(previous.sha256);
    writeFileSync(resolve(app, 'pins.json'), JSON.stringify(rebaked.summary.pins));
    expect((await prove('art-edited-cached')).log).toContain('BUILD cached');
    passed = true;
  } finally {
    if (passed) rmSync(parent, {recursive:true, force:true});
    else console.error(`Retained failed macOS receipt proof: ${parent}`);
  }
}, 600000);
