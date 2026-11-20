import {test, expect} from 'bun:test';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {basename, resolve} from 'node:path';
import {androidJavaEnv, androidSdk, androidSdkPackages, loadAlignment} from './agent-android.mjs';
import {ANDROID_TARGET, androidBakeLibraryName, androidToolchainEnv, stageAndroidBake} from './android-abi.mjs';

test('only development-trust APKs at the dev profile request shrink-only packaging', async () => {
  const {developmentApkArgs} = await import('./agent-android.mjs');
  expect(developmentApkArgs(true, 'host-dev', false)).toEqual(['-PdevelopmentApk=1']);
  expect(developmentApkArgs(true, 'release', false)).toEqual([]);
  expect(developmentApkArgs(false, 'host-dev', false)).toEqual([]);
  expect(developmentApkArgs(true, 'host-dev', true)).toEqual([]);
});

test('an APK carries each GPU library the bake built, by the name Cargo gave it, unstripped (#306)', async () => {
  const { bakeGpuLibraries, stageGpuLibraries } = await import('./agent-android.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-apk-gpu-'));
  try {
    const app = { hasGpu: true, manifest: { gpu: { modules: { extra: ['extra'] } } } };
    // The module's [lib] name is also the preferred name of the data image.
    const products = ['libfiddl_gpu.so', 'libExactBaked.so'].map((file) => resolve(dir, file));
    let asked, image, packagedImage;
    const bake = (...args) => {
      asked = args;
      for (const product of products) expect(args[3].prepareGpu(product)).toBe(product);
      image = args[3].androidPayload();
      args[3].capture({});
    };
    const libraries = bakeGpuLibraries(app, {
      profile: 'host-dev', env: { A: '1' }, androidPayload: true,
      capture: (_, name) => { packagedImage = name; },
    }, bake);
    expect(libraries).toEqual(products);
    expect(asked.slice(0, 3)).toEqual([app, 'android', 'aarch64-linux-android']);
    expect(asked[3]).toMatchObject({ profile: 'host-dev', env: { A: '1' }, kind: 'android' });
    expect(image).toBe('libexactbaked_1.so');
    expect(packagedImage).toBe(image);
    for (const library of libraries) writeFileSync(library, library);
    const jniLibs = resolve(dir, 'jniLibs/arm64-v8a');
    mkdirSync(jniLibs, { recursive: true });
    expect(stageGpuLibraries(libraries, jniLibs)).toEqual(['-PgpuModules=libfiddl_gpu.so,libExactBaked.so']);
    for (const library of libraries) expect(readFileSync(resolve(jniLibs, basename(library)), 'utf8')).toBe(library);
    expect(stageGpuLibraries([], jniLibs)).toEqual([]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

const releaseJavaEnv = androidJavaEnv(process.env);
const releaseSdk = androidSdk(process.env);
test.skipIf(!process.env.ANDROID_NDK_HOME && !existsSync(resolve(releaseSdk, 'ndk')))('a development APK data image maps each edited blob at 16-byte alignment', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-data-'));
  try {
    mkdirSync(resolve(dir, 'plans'));
    const blobs = {'app.plan': 'initial contract', 'compat.json': '{"inputs":{"hermesBytecode":99}}',
      'app.hbc': 'initial module', 'plans/Some-Widget.plan': 'widget plan'};
    const gpuName = 'libExactBaked.so';
    const gpu = resolve(dir, gpuName);
    const link = () => stageAndroidBake(dir, dir, androidToolchainEnv(), ANDROID_TARGET, [{ kind: 'Some-Widget' }],
      androidBakeLibraryName(existsSync(gpu) ? [gpuName] : []));
    const stage = () => {
      blobs['app.module.json'] = JSON.stringify({ module: { file: 'app.hbc', bytes: blobs['app.hbc'].length,
        sha256: createHash('sha256').update(blobs['app.hbc']).digest('hex') } });
      for (const [file, bytes] of Object.entries(blobs)) writeFileSync(resolve(dir, file), bytes);
      return link();
    };
    stage();
    writeFileSync(gpu, 'gpu library');
    blobs['app.plan'] = 'edited contract';
    blobs['app.hbc'] = 'edited module';
    const path = stage(), image = readFileSync(path);
    expect(readFileSync(gpu, 'utf8')).toBe('gpu library');
    expect(loadAlignment(path)).toBeGreaterThanOrEqual(16384);
    for (const bytes of Object.values(blobs)) {
      const at = image.indexOf(bytes);
      expect(at).toBeGreaterThanOrEqual(16);
      expect(at % 16).toBe(0);
      expect(image.readBigUInt64LE(at - 16)).toBe(BigInt(bytes.length));
    }
    expect(image.includes('initial contract')).toBe(false);
    expect(image.includes('initial module')).toBe(false);
    rmSync(resolve(dir, 'app.hbc'));
    expect(link).toThrow('app.hbc');
    rmSync(resolve(dir, 'app.module.json'));
    expect(link).toThrow('app.module.json');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

for (const [id, minimumOS, development = false] of [[undefined, '12'], [undefined, '12', true], ['com.example.android_app', '11'], ['com.example.android_app', '12']]) test.skipIf(process.platform !== 'darwin' || !releaseJavaEnv ||
  !androidSdkPackages().every(pkg => existsSync(resolve(releaseSdk, pkg.path))))(`Android release metadata and signing reach Gradle with ${id ?? 'the default package'} on Android ${minimumOS}, development=${development}`, async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-release-'));
  const packageId = id ?? 'dev.exact.ci';
  const permissions = id ? ['android.permission.BLUETOOTH maxSdkVersion=30', 'android.permission.BLUETOOTH_CONNECT'] : [];
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ app: { id: 'dev.exact.ci', name: 'CI' },
      host: { android: { id, minimumOS } },
      shortcuts: [{ name: 'Open', url: '/open' }] }));
    writeFileSync(resolve(dir, 'upload.jks'), 'configuration fixture');
    const init = resolve(dir, 'inspect.gradle');
    writeFileSync(init, `allprojects { repositories { maven { url = uri('https://127.0.0.1:9/unused-ci-mirror') } } }
    gradle.projectsEvaluated {
      def app = gradle.rootProject.project(':app')
      app.tasks.register('inspectRelease') {
        doLast {
          def android = app.extensions.getByName('android')
          def signing = android.buildTypes.release.signingConfig
          println('EXACT_RELEASE=' + groovy.json.JsonOutput.toJson([
            id: android.defaultConfig.applicationId, suffix: android.defaultConfig.applicationIdSuffix,
            code: android.defaultConfig.versionCode, version: android.defaultConfig.versionName,
            signing: signing.name, store: signing.storeFile.name,
            passwords: signing.storePassword == 'store-secret' && signing.keyPassword == 'key-secret',
            alias: signing.keyAlias]))
        }
      }
    }`);
    const result = spawnSync(resolve(import.meta.dir, '../host/android/gradlew'), ['--no-daemon', '--console=plain', '-I', init,
      ':app:processReleaseMainManifest', ':app:inspectRelease', ...(development ? ['-PdevelopmentApk=1', '-PexactDevelopment=1'] : []), `-PapplicationId=${packageId}.dev`, '-PappName=CI Dev', '-Papp=ci', `-Ppermissions=${permissions.join(',')}`,
      ...(id ? ['-PandroidFeatures=android.hardware.bluetooth_le required=false,android.software.companion_device_setup'] : []), `-PappDir=${dir}`, `-PappBuildDir=${dir}/app`, `-PcanvasBuildDir=${dir}/canvas`, '--project-cache-dir', `${dir}/cache`],
    { cwd: resolve(import.meta.dir, '../host/android'), encoding: 'utf8', timeout: 180000,
      env: { ...releaseJavaEnv, ANDROID_HOME: releaseSdk,
        EXACT_BUILD_NUMBER: '42', EXACT_VERSION: '2.3.4', EXACT_KEYSTORE: resolve(dir, 'upload.jks'),
        EXACT_KEYSTORE_PASSWORD: 'store-secret', EXACT_KEY_ALIAS: 'upload', EXACT_KEY_PASSWORD: 'key-secret' } });
    expect(result.status, result.stdout + result.stderr).toBe(0);
    const line = result.stdout.split('\n').find(line => line.startsWith('EXACT_RELEASE='));
    expect(JSON.parse(line.slice('EXACT_RELEASE='.length))).toEqual({ id: `${packageId}.dev`, suffix: null, code: 42, version: '2.3.4', signing: 'upload', store: 'upload.jks', passwords: true, alias: 'upload' });
    const { readdirSync } = await import('node:fs');
    const manifests = readdirSync(resolve(dir, 'app/intermediates'), { recursive: true })
      .filter(path => /merged_manifest.*AndroidManifest.xml$/.test(path));
    expect(manifests.length).toBeGreaterThan(0);
    const merged = readFileSync(resolve(dir, 'app/intermediates', manifests[0]), 'utf8');
    expect(merged).toContain(`package="${packageId}.dev"`);
    expect(merged).toContain('android:label="CI Dev"');
    expect(merged).not.toContain('android:debuggable="true"');
    expect(merged).toContain(`android:authorities="${packageId}.dev.exact.files"`);
    if (id) {
      if (minimumOS === '11') expect(merged).toMatch(/android:name="android.permission.BLUETOOTH"\s+android:maxSdkVersion="30"/);
      else expect(merged).not.toContain('android:name="android.permission.BLUETOOTH"');
      expect(merged).not.toContain('android.permission.BLUETOOTH_ADMIN');
      expect(merged).toContain('android:name="android.permission.BLUETOOTH_CONNECT"');
      expect(merged).not.toContain('android.permission.BLUETOOTH_SCAN');
      expect(merged).not.toContain('neverForLocation');
      expect(merged).toMatch(/android:name="android.hardware.bluetooth_le"\s+android:required="false"/);
      expect(merged).toMatch(/android:name="android.software.companion_device_setup"\s+android:required="true"/);
      expect(merged).not.toContain('android.hardware.android.');
      expect(merged).not.toContain('android.permission.ACCESS_FINE_LOCATION');
    } else expect(merged).not.toContain('android.permission.BLUETOOTH');
    const shortcuts = readFileSync(resolve(dir, 'app/shortcuts/res/xml/exact_shortcuts.xml'), 'utf8');
    expect(shortcuts).toContain(`android:targetPackage="${packageId}.dev"`);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 200000);

test.skipIf(process.platform !== 'darwin' || !releaseJavaEnv ||
  !androidSdkPackages().every(pkg => existsSync(resolve(releaseSdk, pkg.path))))('development APKs keep shrinking and release artifacts; production keeps full R8 and lint vital', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-packaging-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({app:{id:'dev.exact.ci', name:'CI'}}));
    const init = resolve(dir, 'inspect.gradle');
    writeFileSync(init, `gradle.projectsEvaluated {
      def android = gradle.rootProject.project(':app').extensions.getByName('android')
      def release = android.buildTypes.release
      println('EXACT_PACKAGING=' + groovy.json.JsonOutput.toJson([
        minify: release.minifyEnabled, debuggable: release.debuggable,
        lint: android.lint.checkReleaseBuilds, rules: release.proguardFiles.collect { it.name }]))
    }`);
    const graph = development => spawnSync(resolve(import.meta.dir, '../host/android/gradlew'), [
      '--no-daemon', '--console=plain', '-I', init, '-m', ':app:assembleRelease',
      ...(development ? ['-PdevelopmentApk=1', '-PexactDevelopment=1'] : ['-PexactDevelopment=0']),
      '-PapplicationId=dev.exact.ci', '-PappName=CI', '-Papp=ci', `-PappDir=${dir}`,
      `-PappBuildDir=${dir}/app`, `-PcanvasBuildDir=${dir}/canvas`, '--project-cache-dir', `${dir}/cache`,
    ], {cwd:resolve(import.meta.dir, '../host/android'), encoding:'utf8', timeout:180000,
      env:{...releaseJavaEnv, ANDROID_HOME:releaseSdk}});
    for (const development of [false, true]) {
      const result = graph(development);
      expect(result.status, result.stdout + result.stderr).toBe(0);
      expect(result.stdout).toContain(':app:minifyReleaseWithR8');
      expect(result.stdout).toContain(':canvas:compileReleaseKotlin');
      if (development) expect(result.stdout).not.toContain('lintVital');
      else expect(result.stdout).toContain(':app:lintVitalRelease');
      const line = result.stdout.split('\n').find(line => line.startsWith('EXACT_PACKAGING='));
      const config = JSON.parse(line.slice('EXACT_PACKAGING='.length));
      expect(config).toEqual({minify:true, debuggable:false, lint:!development,
        rules:[...(development ? ['proguard-development.pro'] : []), expect.stringMatching(/^proguard-android-optimize\.txt(?:-.*)?$/), 'proguard-rules.pro']});
    }
    expect(readFileSync(resolve(import.meta.dir, '../host/android/app/proguard-development.pro'), 'utf8').trim())
      .toBe('-dontoptimize\n-dontobfuscate');
  } finally { rmSync(dir, {recursive:true, force:true}); }
}, 600000);

test.skipIf(process.platform !== 'darwin' || !releaseJavaEnv ||
  !androidSdkPackages().every(pkg => existsSync(resolve(releaseSdk, pkg.path))))('one Maven lock write produces a graph the next strict resolution accepts', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-locks-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({app:{id:'dev.exact.locks', name:'Locks'}}));
    const init = resolve(dir, 'resolve.gradle');
    writeFileSync(init, `gradle.projectsEvaluated {
      def app = gradle.rootProject.project(':app')
      app.tasks.register('resolveRuntime') {
        doLast {
          app.configurations.releaseRuntimeClasspath.incoming.resolutionResult.allDependencies.each {
            if (it instanceof org.gradle.api.artifacts.result.UnresolvedDependencyResult) throw it.failure
          }
        }
      }
    }`);
    // Push brings older Kotlin transitives into a graph whose reader selects the current stdlib.
    const {maven} = JSON.parse(readFileSync(resolve(import.meta.dir, '../packages/push/android/exact.json'), 'utf8'));
    const args = ['--no-watch-fs', '--no-daemon', '--console=plain', '-I', init,
      '-Papp=locks', '-PapplicationId=dev.exact.locks', '-PappName=Locks', `-PappDir=${dir}`,
      `-PappBuildDir=${dir}/app`, `-PcanvasBuildDir=${dir}/canvas`, '--project-cache-dir', `${dir}/cache`,
      `-Pmaven=${maven.join(',')}`, `-PmavenLock=${dir}/gradle.lockfile`];
    const run = tasks => spawnSync(resolve(import.meta.dir, '../host/android/gradlew'), [...args, ...tasks], {
      cwd: resolve(import.meta.dir, '../host/android'), encoding:'utf8', timeout:180000, maxBuffer: 1 << 28,
      env:{...releaseJavaEnv, ANDROID_HOME:releaseSdk},
    });
    const written = run([':app:dependencies', ':app:resolveRuntime', '--write-locks']);
    expect(written.status, written.stdout + written.stderr).toBe(0);
    const strict = run([':app:resolveRuntime']);
    expect(strict.status, strict.stdout + strict.stderr).toBe(0);
  } finally { rmSync(dir, {recursive:true, force:true}); }
}, 400000);

test('an APK with a separate bake detects a Contract edit after baking', async () => {
  const { apkChanges } = await import('./agent-stdio.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-apk-bake-inputs-'));
  try {
    const apk = resolve(dir, 'app-release.apk'), library = resolve(dir, 'canvas.so');
    const contract = resolve(dir, 'view.contract'), product = resolve(dir, 'gpu.so'), rust = resolve(dir, 'host.rs');
    const surfaces = resolve(dir, 'surfaces.json');
    const bakeStarted = new Date(10_000), afterBake = new Date(20_000), started = new Date(30_000);
    for (const file of [contract, product, rust, surfaces]) writeFileSync(file, 'input');
    utimesSync(rust, bakeStarted, bakeStarted);
    utimesSync(contract, bakeStarted, bakeStarted);
    utimesSync(product, afterBake, afterBake);
    utimesSync(surfaces, afterBake, afterBake);
    writeFileSync(resolve(dir, 'canvas.d'), `${library}: ${rust}\n`);
    writeFileSync(resolve(dir, 'exact-build.json'), JSON.stringify({
      bakeStarted, started, libraries: [library], workspace: dir, sources: [],
      bakes: [{ paths: [contract, product, surfaces], missing: [], watchedDirectories: [],
        products: [product, surfaces].map(path => ({path, sha256:createHash('sha256').update('input').digest('hex')})) }],
    }));
    expect(apkChanges({ target: resolve(dir, 'target') }, apk)).toEqual([]);
    utimesSync(contract, afterBake, afterBake);
    writeFileSync(surfaces, 'changed');
    utimesSync(surfaces, afterBake, afterBake);
    const changes = apkChanges({ target: resolve(dir, 'target') }, apk);
    expect(changes.map(path => basename(path)).sort()).toEqual(['surfaces.json', 'view.contract']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
