// diagnostics.mjs over a log an interrupted run cut inside its last diagnostic's id.
import { test, expect } from 'bun:test';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

test('a log cut inside an id keeps the diagnostics before it, and the summary finishes', async () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-diagnostics-'));
  try {
    mkdirSync(join(root, 'exact-a', 'session'), { recursive: true });
    const log = join(root, 'exact-a', 'session', 'log.jsonl');
    writeFileSync(log, '{"id":"lower-unknown","message":"no state named x at 3","severity":"error"}\n"id":"lower-');
    const child = Bun.spawn([process.execPath, new URL('./diagnostics.mjs', import.meta.url).pathname, log], { stdout: 'pipe' });
    const timer = setTimeout(() => child.kill(), 3000);
    const [out, code] = await Promise.all([new Response(child.stdout).text(), child.exited]);
    clearTimeout(timer);
    expect(code).toBe(0);
    expect(out).toBe('a | lower-unknown :: no state named x at N\n');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
