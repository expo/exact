// Contract bakes that must rebake, and stay quiet, as their sources and package designs change
// (scripts/app.mjs `buildBake`, `pendingBuildInputs`). Each builds a throwaway Cargo workspace,
// so they run only with EXACT_BAKE_CACHE_TEST=1.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:net';
import { spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, resolve } from 'node:path';
import { bakeTarget, buildBake, pendingBuildInputs } from './app.mjs';

async function withContractBake(use, {binding=false,watchPackage=false}={}) {
  const root=resolve(import.meta.dir,'..'), old=new Date('2000-01-01T00:00:00Z');
  const dir=realpathSync(mkdtempSync(resolve(tmpdir(),'rebake-'))), name='rebake-'+basename(dir).split('-').at(-1).toLowerCase();
  const write=(path, bytes)=>{mkdirSync(dirname(resolve(dir,path)),{recursive:true});writeFileSync(resolve(dir,path),bytes);};
  try {
    write('rust-toolchain.toml',readFileSync(resolve(root,'rust-toolchain.toml')));
    write('Cargo.lock',readFileSync(resolve(root,'Cargo.lock')));
    write('Cargo.toml',`[workspace]\nmembers=["linux"${binding?',"binding"':''}]\nresolver="2"\n[patch.crates-io]\ntaffy={path=${JSON.stringify(resolve(root,'vendor/taffy'))}}\n`);
    write('linux/Cargo.toml',`[package]\nname="${name}-linux"\nversion="0.1.0"\nedition="2021"\n[build-dependencies]\nexact-js-bake={path=${JSON.stringify(resolve(root,'js/bake'))}}\n${binding?`[dependencies]\n${name}-binding={path="../binding"}\n`:''}`);
    write('linux/build.rs',`fn main() {
      ${watchPackage ? 'println!("cargo:rerun-if-changed={}", std::fs::canonicalize("../node_modules/@fixture/ui").unwrap().display());' : ''}
      if std::path::Path::new("../.during-bake").exists() { std::fs::rename("../.during-bake", "../app.contract").unwrap(); }
      exact_js_bake::build(std::path::Path::new(".."), "linux").unwrap();
      std::fs::write(std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("ran"), b"ran").unwrap();
    }`);
    write('linux/src/main.rs',binding?`fn main(){println!("{}", ${name.replaceAll('-','_')}_binding::SOURCE);}`:'fn main(){let _=include_bytes!(concat!(env!("OUT_DIR"),"/app.plan"));}');
    if(binding) {
      write('binding/Cargo.toml',`[package]\nname="${name}-binding"\nversion="0.1.0"\nedition="2021"\n[build-dependencies]\ncontract={path=${JSON.stringify(resolve(root,'contract/cli'))}}\n`);
      write('binding/src/lib.rs','include!(concat!(env!("OUT_DIR"),"/binding.rs"));');
      write('binding/build.rs',`fn main(){
        let path=std::path::Path::new("../app.contract");
        contract::rerun_if_changed(path);
        let plan=contract::compile_path(path).unwrap();
        let source=format!("{:?}\n{}",plan,std::fs::read_to_string(path).unwrap());
        std::fs::write(std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("binding.rs"),format!("pub const SOURCE: &str = {:?};",source)).unwrap();
      }`);
    }
    const id='com.exact.rebake';
    write('app.json',JSON.stringify({app:{id,name:'Rebake'},rust:false,deploy:{store:{linux:'0'}}}));
    write('app.ts',`export const appId='${id}'; export const grants=''; export const answer=()=>{throw new Error('no sources');};`);
    const source=text=>`component App\n  view\n    text "${text}"\n`;
    write('app.contract',source('ORIGINAL')); utimesSync(resolve(dir,'app.contract'),old,old);
    const lock=spawnSync('cargo',['generate-lockfile','--offline'],{cwd:dir,encoding:'utf8'}); assert.equal(lock.status,0,lock.stderr);
    const target=bakeTarget('linux'), output=resolve(dir,'target/bakes');
    const app={dir,workspace:dir,target:process.env.CARGO_TARGET_DIR??resolve(root,'target'),name,id,manifest:{app:{id,name:'Rebake'},rust:false},crate:kind=>`${name}-${kind}`};
    const bake=()=>buildBake(app,'linux',target,{profile:'dev',output,env:{EXACT_UPDATE_TRUST:'development',EXACT_RUST_BUNDLE:undefined,EXACT_UPDATE_RECEIPT:undefined}});
    const plan=()=>readFileSync(resolve(output,`linux-${target}.plan`));
    const run=()=>{const r=spawnSync(resolve(app.target,target,'debug',name+'-linux'),[],{encoding:'utf8'});assert.equal(r.status,0,r.stderr);return r.stdout;};
    const scriptDir=(kind='linux')=>{
      const builds=resolve(app.target,target,'debug/build');
      const unit=readdirSync(builds).find(name=>name.startsWith(app.crate(kind)+'-') && existsSync(resolve(builds,name,kind === 'binding' ? 'out/binding.rs' : 'out/ran')));
      assert.ok(unit, 'the bake must leave its build-script marker');
      return resolve(builds,unit);
    };
    await use({dir,write,source,bake,plan,run,scriptDir,old});
  } finally {rmSync(dir,{recursive:true,force:true});}
}

// Real Cargo and the TypeScript baker must agree even when a restore keeps an old mtime.
test.skipIf(!process.env.EXACT_BAKE_CACHE_TEST)('restored and equal-mtime Contract sources rebake and then stay quiet', async () => {
  await withContractBake(({dir, write, source, bake, plan, scriptDir, old}) => {
    const path = resolve(dir, 'app.contract'), original = resolve(dir, 'app.contract.orig');
    copyFileSync(path, original); utimesSync(original, old, old);
    bake();
    const first = plan(); assert.ok(first.includes(Buffer.from('ORIGINAL')));
    write('app.contract', source('EDITED'));
    bake(); assert.ok(plan().includes(Buffer.from('EDITED')));
    renameSync(original, path);
    bake();
    assert.deepEqual(plan(), first, 'restoring the older source must restore the original plan');
    assert.equal(statSync(path).mtimeMs, old.getTime(), 'the builder must not touch the source');
    write('app.contract', source('EQUALNEW')); utimesSync(path, old, old);
    bake(); assert.ok(plan().includes(Buffer.from('EQUALNEW')), 'changed bytes at the same mtime and size must rebake');
    const ran = resolve(scriptDir(), 'out/ran'), stamp = statSync(ran, {bigint:true}).mtimeNs;
    const expected = plan();
    bake();
    assert.deepEqual(plan(), expected);
    assert.equal(statSync(ran, {bigint:true}).mtimeNs, stamp, 'an unchanged build must not run the bake');
    write('node_modules/@fixture/ui/package.json', JSON.stringify({name:'@fixture/ui', exports:'./child.contract'}));
    write('node_modules/@fixture/ui/child.contract', 'component Child\n  view\n    text "IMPORTED"\n');
    const library = resolve(dir, 'node_modules/@fixture/ui/child.contract');
    utimesSync(library, old, old);
    write('app.contract', 'use Child from "@fixture/ui"\ncomponent App\n  view\n    Child()\n');
    bake(); assert.ok(plan().includes(Buffer.from('IMPORTED')));
    const importedStamp = statSync(ran, {bigint:true}).mtimeNs;
    bake();
    assert.equal(statSync(ran, {bigint:true}).mtimeNs, importedStamp, 'discovering an import must not dirty the next build');
    writeFileSync(library, 'component Child\n  view\n    text "CHANGED!"\n');
    utimesSync(library, old, old);
    bake(); assert.ok(plan().includes(Buffer.from('CHANGED!')), 'installed Contract imports must be watched by content');
  });
}, 300000);

for (const entry of ['socket', 'unreadable file', 'unreadable directory', 'linked unreadable directory']) {
  test.skipIf(!process.env.EXACT_BAKE_CACHE_TEST || process.platform === 'win32' || (entry !== 'socket' && process.getuid?.() === 0))(`an unused ${entry} in a watched package permits quiet bakes`, async()=>{
    await withContractBake(async({dir,write,bake,plan,scriptDir,old})=>{
      write('node_modules/@fixture/ui/package.json',JSON.stringify({name:'@fixture/ui',exports:'./child.contract'}));
      write('node_modules/@fixture/ui/child.contract','component Child\n  view\n    text "PACKAGE_OK"\n');
      write('app.contract','use Child from "@fixture/ui"\ncomponent App\n  view\n    Child()\n');
      const pkg=resolve(dir,'node_modules/@fixture/ui'), cwd=process.cwd();
      mkdirSync(resolve(pkg,'.cache'));
      let server, blocked, payload;
      process.chdir(pkg);
      try {
        if(entry === 'socket') {
          server=createServer();
          await new Promise((ok,fail)=>{server.once('error',fail);server.listen('.cache/worker.sock',ok);});
        } else {
          const file=entry === 'unreadable file';
          blocked=entry === 'linked unreadable directory' ? resolve(dir,'.private') : resolve(pkg,'private');
          if(!file)mkdirSync(blocked);
          payload=file ? blocked : resolve(blocked,'payload');
          writeFileSync(payload,'before');utimesSync(payload,old,old);
          if(entry === 'linked unreadable directory')symlinkSync(blocked,resolve(pkg,'link'),'dir');
          chmodSync(blocked,0);
          assert.throws(()=>file ? readFileSync(blocked) : readdirSync(blocked), error=>['EACCES','EPERM'].includes(error.code));
        }
        const build = bake(); assert.ok(plan().includes(Buffer.from('PACKAGE_OK')));
        assert.ok(build.watchedDirectories.some(({path}) => path === pkg), 'a watched package must retain its entry inventory');
        // This fixture explicitly watches the directory, including its unused entries.
        const watches=readFileSync(resolve(scriptDir(),'output'),'utf8').split('\n');
        assert.ok(watches.includes(`cargo:rerun-if-changed=${pkg}`),'the package directory must actually be watched');
        const stamp=()=>statSync(resolve(scriptDir(),'out/ran'),{bigint:true}).mtimeNs;
        const first=stamp(), expected=plan();
        bake(); assert.deepEqual(plan(),expected);
        assert.equal(stamp(),first,'an unchanged package must not rerun the bake');
        if(blocked) {
          chmodSync(blocked,entry === 'unreadable file' ? 0o600 : 0o700);
          writeFileSync(payload,'after!');utimesSync(payload,old,old);
          bake(); assert.deepEqual(plan(),expected);
          const readable=stamp();
          assert.notEqual(readable,first,'a now-readable input must change the revision even at an old mtime');
          bake(); assert.equal(stamp(),readable,'a readable unchanged package must stay quiet');
        }
      } finally {
        if(blocked)chmodSync(blocked,0o700);
        if(server)await new Promise(ok=>server.close(ok));
        process.chdir(cwd);
      }
    }, {watchPackage:true});
  },300000);
}

test.skipIf(!process.env.EXACT_BAKE_CACHE_TEST)('a dependency regenerates Rust when the contract is restored',async()=>{
  await withContractBake(({dir,write,source,bake,plan,run,scriptDir,old})=>{
    const path=resolve(dir,'app.contract'), saved=resolve(dir,'.saved');
    copyFileSync(path,saved);utimesSync(saved,old,old);
    bake(); assert.ok(run().includes('ORIGINAL'));
    write('app.contract',source('EDITED'));
    bake(); assert.ok(run().includes('EDITED'));
    renameSync(saved,path);bake();
    assert.ok(plan().includes(Buffer.from('ORIGINAL')));
    assert.ok(run().includes('ORIGINAL'),'the compiled dependency must agree with the restored plan');
    write('node_modules/@fixture/ui/package.json', '{"name":"@fixture/ui","exports":"./child.contract"}');
    write('node_modules/@fixture/ui/child.contract', 'component Child\n  view\n    text "DEPENDENCY"\n');
    write('binding/data/Cargo.toml', '[package]\nname="binding-data"\n');
    write('binding/entry.contract', 'use Child from "@fixture/ui"\ncomponent App\n  view\n    Child()\n');
    write('binding/build.rs', readFileSync(resolve(dir, 'binding/build.rs'), 'utf8').replace('../app.contract', 'entry.contract'));
    bake(); const before = run();
    const generated = resolve(scriptDir('binding'), 'out/binding.rs'), stamp = statSync(generated, {bigint:true}).mtimeNs;
    bake(); assert.equal(statSync(generated, {bigint:true}).mtimeNs, stamp, 'a dependency with an absent design must stay warm');
    // Only the dependency reads this package. The app's revision stays fixed.
    write('node_modules/@fixture/ui/ui.tamber', 'tamber ui\n\ntheme\n  ui.card = #123456\n');
    bake(); assert.notEqual(run(), before, 'a dependency must regenerate when its optional design appears');
  },{binding:true});
},300000);

test.skipIf(!process.env.EXACT_BAKE_CACHE_TEST)('the app\'s absent optional design, which Cargo does not watch, is a plan input, not a binary input', async () => {
  await withContractBake(({dir, write, bake}) => {
    write('node_modules/@fixture/ui/package.json', JSON.stringify({name: '@fixture/ui', exports: './child.contract'}));
    write('node_modules/@fixture/ui/child.contract', 'component Child\n  view\n    text "DESIGNED"\n');
    write('app.contract', 'use Child from "@fixture/ui"\ncomponent App\n  view\n    Child()\n');
    const design = resolve(dir, 'node_modules/@fixture/ui/ui.tamber');
    const absent = bake();
    assert.ok(absent.optionalInputs?.includes(design) && !absent.binary.missing.includes(design));
    assert.deepEqual(pendingBuildInputs(absent), []);
    write('node_modules/@fixture/ui/ui.tamber', 'tamber ui\n\ntheme\n  ui.card = #123456\n');
    assert.deepEqual(pendingBuildInputs(absent), [design], 'making the design must make the build stale');
    const present = bake();
    assert.equal(present.binary.sha256, absent.binary.sha256, 'making the design must not change the binary');
  });
}, 300000);

test.skipIf(!process.env.EXACT_BAKE_CACHE_TEST)('restoring bytes seen before an in-build save rebakes',async()=>{
  await withContractBake(({dir,write,source,bake,plan,old})=>{
    bake();
    write('app.contract',source('BEFORE'));utimesSync(resolve(dir,'app.contract'),old,old);
    write('.during-bake',source('DURING'));utimesSync(resolve(dir,'.during-bake'),old,old);
    bake();assert.ok(plan().includes(Buffer.from('DURING')));
    write('app.contract',source('BEFORE'));utimesSync(resolve(dir,'app.contract'),old,old);
    bake();assert.ok(plan().includes(Buffer.from('BEFORE')),'restored pre-build bytes must not reuse the consumed revision');
  });
},300000);
