import { beforeAll, test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { BUILD_SCRIPT_FAILED_LINE, contractMayHaveFailed, throwContractErrors } from './contract-diagnosis.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const failed = (stderr, stdout = '') => ({ status: 101, stderr, stdout });
const rustcError = '{"reason":"compiler-message","package_id":"x","message":{"level":"error","message":"linking with `cc` failed"}}';
const warning = '{"reason":"compiler-message","package_id":"x","message":{"level":"warning","message":"unused"}}';

test('only a failed build script asks the Contract compiler', () => {
  const script = 'error: failed to run custom build command for `brooks-apple v0.1.0`\n  app.contract:3:1 [lower-unknown-attr] …';
  assert.equal(contractMayHaveFailed(['build', '-p', 'app'], failed(script)), true);
  assert.equal(contractMayHaveFailed(['rustc', '--crate-type', 'staticlib'], failed(script)), true);
  // A build script that logs `cargo::error=` and returns, as Cargo 1.99 says it
  assert.equal(contractMayHaveFailed(['build'], failed('error: app@0.1.0: app.contract:3:1 [lower-unknown-attr] …\n\x1b[1m\x1b[91merror\x1b[0m: build script logged errors')), true);
  // rustc's and the linker's own errors, and commands that run no build script
  assert.equal(contractMayHaveFailed(['build'], failed('error[E0308]: mismatched types')), false);
  assert.equal(contractMayHaveFailed(['build'], failed('error: linking with `cc` failed: exit status: 1')), false);
  assert.equal(contractMayHaveFailed(['metadata', '--format-version', '1'], failed(script)), false);
  assert.equal(contractMayHaveFailed(['build'], { error: new Error('spawnSync cargo ENOENT'), status: null, stderr: '' }), false);
  assert.equal(contractMayHaveFailed(['build'], { status: 0, stderr: '', stdout: '' }), false);
});

// With stderr inherited, only the relay's line says a build script failed: Cargo's JSON carries no rustc error under
// json-render-diagnostics, and its human format nothing.
test('an inherited stderr is read from the line its relay adds', () => {
  const finished = '{"reason":"build-finished","success":false}';
  assert.equal(contractMayHaveFailed(['build'], failed(null, `${warning}\n${finished}\n${BUILD_SCRIPT_FAILED_LINE}\n`)), true);
  assert.equal(contractMayHaveFailed(['build'], failed(null, `${warning}\n${rustcError}\n${finished}\n`)), false);
  assert.equal(contractMayHaveFailed(['build'], failed(null, '')), false);
});

// The relay streams Cargo's stderr and adds the line when its header says a build script failed, however the stream
// splits it, on a failed exit; not for the phrase quoted in rustc's excerpt of a type error's source line.
test('the stderr relay adds the line for a failed build script, and only then', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-relay-'));
  try {
    const cargo = resolve(dir, 'cargo');
    writeFileSync(cargo, `#!/bin/sh
printf '{"reason":"build-finished","success":false}\\n'
case "$1" in
  script) printf 'error: failed to ' >&2; sleep 0.1; printf 'run custom ' >&2; sleep 0.1; printf 'build command for \`app\`\\n' >&2; exit 101 ;;
  quoted) printf '2 |     let _: u32 = "failed to run custom build command";\\nerror[E0308]: mismatched types\\n' >&2; exit 101 ;;
  passed) printf 'error: failed to run custom build command for \`app\`\\n' >&2; exit 0 ;;
esac
`);
    chmodSync(cargo, 0o755);
    const relay = (arg) => spawnSync(process.execPath, ['--no-env-file', resolve(ROOT, 'scripts/cargo-check.mjs'), arg],
      { encoding: 'utf8', env: { ...process.env, PATH: `${dir}:${process.env.PATH}` } });
    const marked = (arg) => relay(arg).stdout.split('\n').includes(BUILD_SCRIPT_FAILED_LINE);
    assert.deepEqual([marked('script'), marked('quoted'), marked('passed')], [true, false, false]);
    assert.equal(contractMayHaveFailed(['build'], failed('2 |     let _: u32 = "failed to run custom build command";\nerror[E0308]: mismatched types')), false);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// The compiler is built before the case, on its own budget: a slow or failed
// build is said as one, not as the case's timeout.
beforeAll(() => {
  const built = spawnSync('cargo', ['build', '-q', '-p', 'contract', '--bin', 'contract'], { cwd: ROOT, encoding: 'utf8' });
  if (built.status !== 0) throw new Error(`building the Contract compiler failed:\n${built.stderr}`);
}, 600_000);

test('a Contract error in a failed build script is said with every error', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-contract-diagnosis-'));
  try {
    writeFileSync(resolve(dir, 'app.contract'), 'screen Home\n');
    const app = { name: 'broken', dir };
    // A linker failure in the same app leaves the build's own error to the caller.
    assert.doesNotThrow(() => throwContractErrors(app, ['build'], failed('error: linking with `cc` failed'), ROOT));
    assert.throws(() => throwContractErrors(app, ['build'], failed('error: failed to run custom build command for `broken-apple`'), ROOT),
      (error) => error.contract === true && /broken: the Contract does not compile:\n {2}\S*app\.contract:\d+:\d+ \[/.test(error.message));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
