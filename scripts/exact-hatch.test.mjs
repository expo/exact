// `exact hatch` (LLP 1075.003.000.001 §5). `bun test ./scripts/exact-hatch.test.mjs`.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';

// LLP 1075.003.000.001 §5: `exact hatch` writes a stub a target, wires it into a module it wrote, and declares the word.
test('exact hatch writes each target\'s stub, wires it in, and declares the word with its platforms', async () => {
  const { hatch } = await import('./exact.mjs');
  const { readFileSync } = await import('node:fs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-hatch-'));
  const was = process.env.EXACT_APP_DIR;
  try {
    for (const d of ['apple', 'web']) mkdirSync(resolve(dir, d));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    column hatch="avatar"\n');
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Scratch', app: { id: 'com.example.scratch', name: 'Scratch' }, host: { ios: {}, web: {} } }));
    process.env.EXACT_APP_DIR = dir;
    const said = [];
    const first = hatch(['avatar'], line => said.push(line));
    assert.deepEqual(first.platforms, ['ios', 'web']);
    assert.deepEqual(JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8')).hatches, { avatar: ['ios', 'web'] });
    const swift = readFileSync(resolve(dir, 'modules/apple/Hatches.swift'), 'utf8');
    assert.ok(swift.includes('if element.hatch == .avatar { avatarHatch(element) }') && swift.includes('if element.hatch == .avatar { avatarHatchEnded(element) }'));
    assert.ok(readFileSync(resolve(dir, 'modules/apple/AvatarHatch.swift'), 'utf8').includes('func avatarHatch(_ element: ExactElement)'));
    const page = readFileSync(resolve(dir, 'modules/web/index.js'), 'utf8');
    assert.ok(page.includes("import * as avatarHatch from './hatch-avatar.js';") && page.includes('hatches["avatar"] = avatarHatch;'));
    assert.ok(readFileSync(resolve(dir, 'modules/web/hatch-avatar.js'), 'utf8').includes('export function elementEnded(e)'));
    // A second word joins the first; the same word again changes nothing; the scopes take no word. The module's
    // directories and files other than Swift sources are not read.
    mkdirSync(resolve(dir, 'modules/apple/0-support.swift'), { recursive: true });
    writeFileSync(resolve(dir, 'modules/apple/0-icon.png'), Buffer.from([0x89, 0x50, 0x4e, 0x47]));
    hatch(['unread-dot'], () => {});
    const again = hatch(['avatar'], () => {});
    assert.deepEqual(again.wrote, []);
    assert.deepEqual(JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8')).hatches, { avatar: ['ios', 'web'], 'unread-dot': ['ios', 'web'] });
    assert.ok(readFileSync(resolve(dir, 'modules/apple/Hatches.swift'), 'utf8').includes('if element.hatch == .unreadDot { unreadDotHatch(element) }'));
    hatch(['--window'], () => {});
    assert.ok(readFileSync(resolve(dir, 'modules/apple/Hatches.swift'), 'utf8').includes('        windowHatch(window)\n        // exact:window\n'));
    assert.ok(readFileSync(resolve(dir, 'modules/web/index.js'), 'utf8').includes('export function window(x) { windowHatch.built(x); }'));
    // The app hatch reads the root's data-* words, which app.json `data` declares: the verb adds the list and names the accessor.
    const app = [];
    hatch(['--app'], line => app.push(line));
    assert.deepEqual(JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8')).data, []);
    assert.ok(app.some(line => line.includes('app.data[.booted]')), app.join('\n'));
    assert.ok(readFileSync(resolve(dir, 'modules/apple/AppHatch.swift'), 'utf8').includes('app.data[.booted]'));
    assert.throws(() => hatch(['Not A Word'], () => {}), /name a word/);
    // An app that names Linux, whose crate is generated (.exact/shells), gets one hatches file, which that crate's
    // bake includes: nothing is owed, and its build script is made to run again.
    const manifest = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8'));
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ ...manifest, host: { ...manifest.host, linux: {} } }));
    const shell = resolve(dir, '.exact/shells/linux/build.rs');
    mkdirSync(resolve(shell, '..'), { recursive: true });
    writeFileSync(shell, 'fn main() {}\n');
    utimesSync(shell, 0, 0);
    const generated = hatch(['unread-dot'], () => {});
    assert.ok(generated.platforms.includes('linux') && generated.wrote.some(file => file.endsWith('/modules/linux/hatches.rs')));
    assert.deepEqual(generated.todo, []);
    assert.ok(statSync(shell).mtimeMs > 0);
    rmSync(resolve(dir, 'modules/linux'), { recursive: true });
    // An authored Linux crate gets the same file, and is told what its build.rs owes.
    mkdirSync(resolve(dir, 'linux'));
    const linux = hatch(['unread-dot'], () => {});
    assert.ok(linux.platforms.includes('linux') && linux.todo.some(line => line.includes('rust_hatch_entry')));
    const rust = readFileSync(resolve(dir, 'modules/linux/hatches.rs'), 'utf8');
    assert.ok(rust.includes('"unread-dot" => unread_dot_hatch(element, context),') && rust.includes('fn unread_dot_hatch_ended<H: Hatches>'));
    assert.deepEqual(JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8')).hatches['unread-dot'], ['ios', 'web', 'linux']);
    // A build.rs that already asks for the app's hatches owes nothing: the first file only makes it run again.
    rmSync(resolve(dir, 'modules/linux'), { recursive: true });
    writeFileSync(resolve(dir, 'linux/build.rs'), 'fn main() { let _ = contract::native::rust_hatch_entry; }\n');
    assert.deepEqual(hatch(['unread-dot'], () => {}).todo.filter(line => line.includes('build.rs')), []);
    // An Android crate gets one Kotlin class, each word an arm and its functions, and is told what its build.rs owes.
    mkdirSync(resolve(dir, 'android'));
    const android = hatch(['unread-dot'], () => {});
    assert.ok(android.platforms.includes('android') && android.todo.some(line => line.includes('android_entry')));
    const kotlin = readFileSync(resolve(dir, 'modules/android/Hatches.kt'), 'utf8');
    assert.ok(kotlin.includes('class Hatches : ExactHatches()') && kotlin.includes('"unread-dot" -> unreadDot(element)') && kotlin.includes('"unread-dot" -> unreadDotEnded(element)'));
    assert.ok(JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8')).hatches['unread-dot'].includes('android'));
    writeFileSync(resolve(dir, 'android/build.rs'), 'fn main() { let _ = contract::native::android_entry; }\n');
    assert.deepEqual(hatch(['unread-dot'], () => {}).todo.filter(line => line.includes('android/build.rs')), []);
    // In a module of the app's own, with no marker, the lines to add are shown, not written.
    rmSync(resolve(dir, 'modules/apple'), { recursive: true });
    mkdirSync(resolve(dir, 'modules/apple/0-support'), { recursive: true });
    writeFileSync(resolve(dir, 'modules/apple/Mine.swift'), 'final class Mine: ExactModule {}\nlet exactModule: ExactModule.Type = Mine.self\n');
    const mine = hatch(['seal'], () => {});
    assert.equal(mine.todo.length, 2);
    assert.ok(mine.todo.some(line => line.includes('if element.hatch == .seal { sealHatch(element) }')));
    assert.equal(readFileSync(resolve(dir, 'modules/apple/Mine.swift'), 'utf8').includes('sealHatch'), false);
  } finally {
    if (was === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = was;
    rmSync(dir, { recursive: true, force: true });
  }
});
