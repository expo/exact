import {test, expect} from 'bun:test';
import {spawnSync} from 'node:child_process';
import {existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve} from 'node:path';
import {androidJavaEnv, androidSdk, androidSdkPackages} from './agent-android.mjs';

test('only development-trust APKs at the dev profile request shrink-only packaging', async () => {
  const {developmentApkArgs} = await import('./agent-android.mjs');
  expect(developmentApkArgs(true, 'host-dev', false)).toEqual(['-PdevelopmentApk=1']);
  expect(developmentApkArgs(true, 'release', false)).toEqual([]);
  expect(developmentApkArgs(false, 'host-dev', false)).toEqual([]);
  expect(developmentApkArgs(true, 'host-dev', true)).toEqual([]);
});

const releaseJavaEnv = androidJavaEnv(process.env);
const releaseSdk = androidSdk(process.env);
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
