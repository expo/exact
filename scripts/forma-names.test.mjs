import { expect, test } from 'bun:test';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { captureSwiftNames, wantsSwiftNames, writeSwiftNames } from './forma-names.mjs';

test('Swift names belong to one successful bake and survive later OUT_DIR writes', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'forma-names-'));
  try {
    const hash = value => createHash('sha256').update(value).digest('hex');
    const file = resolve(dir, 'forma.swift');
    const plan = Buffer.from('baked plan'), names = 'Swift bindings';
    const record = `cargo:metadata=exact-forma-swift=${hash(plan)}:${hash(names)}\n`;
    const read = (output = record, digest = hash(plan)) => captureSwiftNames(dir, output, digest);
    expect(read()).toBeUndefined();
    writeFileSync(file, names);
    expect(read('')).toBeUndefined();
    expect(read(record + record)).toBeUndefined();
    expect(read(record, hash('another plan'))).toBeUndefined();
    const captured = read();
    writeFileSync(file, 'stale');
    expect(read()).toBeUndefined();
    const scratch = resolve(dir, 'scratch');
    const design = resolve(dir, 'app.forma');
    writeFileSync(design, 'forma test');
    const destination = writeSwiftNames({ dir }, captured, plan, scratch);
    expect(readFileSync(destination, 'utf8')).toBe(names);
    const publicFile = resolve(dir, '.exact/forma/forma.swift');
    expect(readFileSync(publicFile, 'utf8')).toBe(names);
    writeFileSync(publicFile, 'another build');
    expect(readFileSync(destination, 'utf8')).toBe(names);
    const original = statSync(destination, { bigint: true }).mtimeNs;
    writeSwiftNames({ dir }, captured, plan, scratch);
    expect(statSync(destination, { bigint: true }).mtimeNs).toBe(original);
    // Removing the design invalidates a projection even for the same plan.
    rmSync(design);
    expect(existsSync(writeSwiftNames({ dir }, captured, plan, scratch))).toBe(false);
    expect(existsSync(destination)).toBe(false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('only Apple hosts with known Swift sources and an existing design request names', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'forma-request-'));
  try {
    const app = { dir }, sources = ['View.swift'];
    expect(wantsSwiftNames(app, sources, false)).toBe(false);
    writeFileSync(resolve(dir, 'app.forma'), 'forma test');
    expect(wantsSwiftNames(app, sources, false)).toBe(true);
    expect(wantsSwiftNames(app, sources, true)).toBe(false);
    expect(wantsSwiftNames(app, [], false)).toBe(false);
    app.manifest = { forma: 'other.forma' };
    expect(wantsSwiftNames(app, sources, false)).toBe(false);
    writeFileSync(resolve(dir, 'other.forma'), 'forma other');
    expect(wantsSwiftNames(app, sources, false)).toBe(true);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
