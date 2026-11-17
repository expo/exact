// The XCTests' fixture prep (compileFixtures) into a directory outside the checkout, as a CARGO_TARGET_DIR there
// makes it. A fixture it derives and compiles, notification-no-actions.contract, imports `@exact/notifications`,
// which resolves from a node_modules above the source; written beside its plan out there, it was refused ("no
// package `@exact/notifications` in a `node_modules` above …") and every Mac and iOS XCTest run stopped at prep.
import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { compileFixtures } from './xctest.mjs';

const ROOT = resolve(import.meta.dir, '../..');
const CONTRACT = resolve(process.env.CARGO_TARGET_DIR ?? resolve(ROOT, 'target'), 'debug', 'contract');

test.skipIf(process.platform !== 'darwin')('fixture prep compiles every plan into a directory outside the checkout, over a stale app.tamber', () => {
  expect(existsSync(CONTRACT), `${CONTRACT}: build it with cargo build -p contract --bin contract`).toBe(true);
  const out = mkdtempSync(resolve(tmpdir(), 'exact-fixture-prep-'));
  try {
    const run = (cmd, args, opts = {}) => {
      const r = spawnSync(cmd, args, { cwd: ROOT, encoding: 'utf8', ...opts });
      if (r.status !== 0) throw new Error(`${cmd} ${args.slice(0, 2).join(' ')} failed (${r.status ?? r.error?.message}): ${r.stderr}`);
      return r;
    };
    // A target from before the tamber->forma rename keeps the icon entry's app.tamber, which contract refuses.
    mkdirSync(resolve(out, 'entries/raster-icons'), { recursive: true });
    writeFileSync(resolve(out, 'entries/raster-icons/app.tamber'), 'tamber icons\n');
    const env = { EXACT_CONTRACT: CONTRACT };
    compileFixtures(ROOT, run, env, out, 'EXACT', 'com.exact.testhost');
    expect(env.EXACT_NOTIFICATION_NO_ACTIONS_PLAN).toBe(resolve(out, 'notification-no-actions.plan'));
    expect(existsSync(env.EXACT_NOTIFICATION_NO_ACTIONS_PLAN)).toBe(true);
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 300_000);
