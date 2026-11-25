# `simulatorApps` and `exclusiveIOS` default to `tmpdir()` instead of `simulatorTempDirectory()`, breaking simulator app exclusion locks across processes with different `TMPDIR`s

**Status:** Open
**Systems:** iOS simulator drive & install locking
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Check, then act)

Verification: verified by running.

At `9c0431fd5`, `simulatorApps` (`scripts/agent-launch.mjs:1430`) and `exclusiveIOS` (`scripts/agent-launch.mjs:1443`) still default their lock `directory` to `resolve(tmpdir(), 'exact-ios-drives')`, whereas `simulatorDisplayLockPath` (`:1404`) and `simulatorDisplay` (`:1423`) right above them use `resolve(simulatorTempDirectory(), 'exact-ios-drives')`:

```js
export const simulatorDisplayLockPath = udid => resolve(simulatorTempDirectory(), 'exact-ios-drives', simulatorLock(['display', udid]));
...
export async function simulatorDisplay(udid, fn, { directory = resolve(simulatorTempDirectory(), 'exact-ios-drives'), timeout = 60000 } = {}) { ... }
export async function simulatorApps(udid, bundles, fn, { name = udid, directory = resolve(tmpdir(), 'exact-ios-drives'), timeout = 60000 } = {}) { ... }
export async function exclusiveIOS(udid, bundle, launch, { directory = resolve(tmpdir(), 'exact-ios-drives'), timeout = 60000 } = {}) { ... }
```

### Triggering sequence
Run `bun scripts/agent.mjs ios ...` (which calls `exclusiveIOS(dev.udid, id, ...)` without `directory` at `scripts/agent-ios.mjs:70`) concurrently with another `agent ios` drive or `bun host/apple/build.mjs --ios --run` (which calls `simulatorApps(dev.udid, bundles, place, { name: dev.name })` without `directory` at `host/apple/build.mjs:1398`) targeting the same simulator and bundle ID from two processes with different `TMPDIR` values (for example, sandboxed tools, worktrees, or a shorter `TMPDIR` as `scripts/agent-launch.mjs:414-415` instructs).

### Actual vs. expected
- **Actual:** Each process creates and locks its `.lock` file inside its own `$TMPDIR/exact-ios-drives/<sha256>/.lock`, providing no mutual exclusion across different `TMPDIR` values. A concurrent `ios --run` or second drive immediately installs over or terminates (`simctl launch --terminate-running-process` at `host/apple/build.mjs:1394`) the active drive's simulator app mid-run.
- **Expected:** Because CoreSimulator is shared across processes with different `TMPDIR`s (`host/apple/devices.mjs:397-400`, commit `e1e388a9`) and each simulator runs one process per bundle ID across every checkout (`scripts/agent-launch.mjs:1427-1429, 1440-1442`), `simulatorApps` and `exclusiveIOS` should default `directory` to `resolve(simulatorTempDirectory(), 'exact-ios-drives')`, matching `simulatorDisplay` and `simulatorDisplayLockPath`.

### Verification & siblings checked
- **Verified by running:** Spawned a Bun process holding `exclusiveIOS('probe-sim-<pid>', 'com.example.probe', ...)` with `TMPDIR=<d1>`, then invoked `simulatorApps` and `exclusiveIOS` with `timeout: 0` from child processes using `TMPDIR=<d1>` and `TMPDIR=<d2>`. Under `TMPDIR=<d1>`, `simulatorApps` failed immediately with the lock-busy error (exit 1); under `TMPDIR=<d2>`, both `simulatorApps` and `exclusiveIOS` succeeded concurrently (`{"gotApp":true,"gotExcl":true}`, exit 0).
- **Siblings checked:**
  - `scripts/agent-launch.mjs:1404` (`simulatorDisplayLockPath`) and `:1423` (`simulatorDisplay`) — already use `simulatorTempDirectory()`.
  - `host/apple/touches.mjs:151` (`touchLockPath`) and `:294` (`hostedAccessibilityLock`) — already use `simulatorTempDirectory()`. Note that `host/apple/touches.mjs:197` (`keptPath`) still stores the kept touch runner record under `tmpdir()` rather than `simulatorTempDirectory()`, which similarly splits runner discovery across different `TMPDIR`s even while its lock is unified.
  - `scripts/agent-launch.mjs:254` (`exclusiveNativeStore`) and `host/apple/devices.mjs:571, 588` (`softwareKeyboard` / `withSoftwareKeyboard`) — store locks/records under `~/.cache/exact/...`, unaffected by `TMPDIR`.

### Suggested fix & regression test
Default `directory` in both `simulatorApps` (`scripts/agent-launch.mjs:1430`) and `exclusiveIOS` (`scripts/agent-launch.mjs:1443`) to `resolve(simulatorTempDirectory(), 'exact-ios-drives')` (sharing a helper with `simulatorDisplay` and `simulatorDisplayLockPath`). Add a regression test in `host/web/agent.test.mjs` (alongside the existing `exclusiveIOS`/`simulatorApps` tests at lines 1352–1408) spawning two child processes with distinct `TMPDIR` environment variables and asserting that default-directory `exclusiveIOS` blocks concurrent `simulatorApps` and `exclusiveIOS` on the same `(udid, bundle)`.

### Siblings checked

scripts/agent-launch.mjs:1404,1423 (simulatorDisplayLockPath/simulatorDisplay already use simulatorTempDirectory()); host/apple/touches.mjs:151,294 (use simulatorTempDirectory(); note keptPath at line 197 still uses tmpdir()); scripts/agent-launch.mjs:254 and host/apple/devices.mjs:571,588 (use ~/.cache/exact)
