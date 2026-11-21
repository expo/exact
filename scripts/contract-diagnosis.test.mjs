import { beforeAll, test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { contractMayHaveFailed, throwContractErrors } from './contract-diagnosis.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const failed = (stderr, stdout = '') => ({ status: 101, stderr, stdout });
const rustcError = '{"reason":"compiler-message","package_id":"x","message":{"level":"error","message":"linking with `cc` failed"}}';
const warning = '{"reason":"compiler-message","package_id":"x","message":{"level":"warning","message":"unused"}}';

test('only a failed build script asks the Contract compiler', () => {
  const script = 'error: failed to run custom build command for `brooks-apple v0.1.0`\n  app.contract:3:1 [lower-unknown-attr] …';
  assert.equal(contractMayHaveFailed(['build', '-p', 'app'], failed(script)), true);
  assert.equal(contractMayHaveFailed(['rustc', '--crate-type', 'staticlib'], failed(script)), true);
  // rustc's and the linker's own errors, and commands that run no build script
  assert.equal(contractMayHaveFailed(['build'], failed('error[E0308]: mismatched types')), false);
  assert.equal(contractMayHaveFailed(['build'], failed('error: linking with `cc` failed: exit status: 1')), false);
  assert.equal(contractMayHaveFailed(['metadata', '--format-version', '1'], failed(script)), false);
  assert.equal(contractMayHaveFailed(['build'], { error: new Error('spawnSync cargo ENOENT'), status: null, stderr: '' }), false);
  assert.equal(contractMayHaveFailed(['build'], { status: 0, stderr: '', stdout: '' }), false);
});

// Cargo's messages for `cargo build --message-format=json-render-diagnostics` of an app whose build script bakes the
// plan: the script built and, when it ran, executed; a rustc error is rendered to stderr and is not among them.
const script = '{"reason":"compiler-artifact","package_id":"app","target":{"kind":["custom-build"],"name":"build-script-build"}}';
const executed = '{"reason":"build-script-executed","package_id":"app"}', finished = '{"reason":"build-finished","success":false}';

test('an inherited stderr is read from the JSON messages on stdout', () => {
  assert.equal(contractMayHaveFailed(['rustc'], failed(null, `${warning}\n${rustcError}\n`)), false);
  // The script built and did not run: it failed, and the Contract may be why.
  assert.equal(contractMayHaveFailed(['build'], failed(null, `${script}\n${finished}\n`)), true);
  // The script ran and the crate's own code then failed, as a type error does: no Contract to ask.
  assert.equal(contractMayHaveFailed(['build'], failed(null, `${script}\n${executed}\n${warning}\n${finished}\n`)), false);
  assert.equal(contractMayHaveFailed(['rustc'], failed(null, `${warning}\n${finished}\n`)), false);
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
