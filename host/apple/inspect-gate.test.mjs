// Inspect mode and the agent's private API are compiled out of a production
// bake (LLP 1122 D5, T2c; theme2's ruling; LLP 1035.003 §3):
// `bun test host/apple/inspect-gate.test.mjs`; the async lane runs it
// (host/apple/tests/it/development.rs). Package.swift
// defines `EXACT_INSPECT` for every trust policy but production, and every
// Swift line naming inspect mode's types or the agent's private calls is
// inside an `#if` on it, so a production build compiles none of it. Shipped
// code reads no undocumented defaults key, notification or string table. A
// production Mac bake on disk (from
// `EXACT_UPDATE_TRUST=production bun host/apple/build.mjs theme-fixture-apple`)
// is checked for their strings too.
import { test, expect } from 'bun:test';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { spawnSync as appleSpawnSync } from './toolchain.mjs';

const root = resolve(import.meta.dir, '../..');
const inspectNames = /\b(Inspect|InspectHost|InspectOverlay)\b|\binspect(Command|Tapped)\b|\bleaveInspect\b|\bgroupId\b|\bshownHeader\b|\bitemNodes\b/;

function swiftFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => e.isDirectory() ? swiftFiles(join(dir, e.name)) : e.name.endsWith('.swift') ? [join(dir, e.name)] : []);
}

test('Package.swift defines EXACT_INSPECT for development and not for production', () => {
  const defines = (trust) => {
    const out = appleSpawnSync('swift', ['package', 'dump-package'], { cwd: resolve(root, 'host/apple'), env: { ...process.env, EXACT_UPDATE_TRUST: trust }, encoding: 'utf8' });
    expect(out.status).toBe(0);
    return out.stdout.includes('"EXACT_INSPECT"');
  };
  expect(defines('development')).toBe(true);
  expect(defines('production')).toBe(false);
});

/** Whether an `#if` or `#elseif` condition holds only where one of `flags`
 * is defined: a conjunction with one of them as a bare term. A condition with
 * `||`, a negated group or a block comment is taken to hold in production too. */
function onlyUnder(condition, flags) {
  if (/\|\||!\s*\(|\/\*/.test(condition)) return false;
  return condition.split('&&').some((term) => flags.includes(term.replace(/[()\s]/g, '')));
}

/** The lines of one Swift file outside the `#if` branches only `flags`
 * compile, each with its number. A directive is read without its comment. */
function linesOutside(text, flags) {
  const lines = [], stack = [];
  text.split('\n').forEach((line, i) => {
    const t = line.trim(), directive = t.replace(/\/\/.*$/, '').trim();
    if (/^#if\s/.test(directive)) stack.push(onlyUnder(directive.slice(3), flags));
    else if (/^#elseif\s/.test(directive)) stack[stack.length - 1] = onlyUnder(directive.slice(7), flags);
    else if (/^#else\b/.test(directive)) stack[stack.length - 1] = false;
    else if (/^#endif\b/.test(directive)) stack.pop();
    else if (!t.startsWith('//') && !stack.includes(true)) lines.push({ line: i + 1, code: line });
  });
  return lines;
}

/** Each line of the Swift under `host/apple/Sources` that matches `names`
 * outside an `#if EXACT_INSPECT`, quoted strings left in when `strings`. */
function outsideInspect(names, { strings = false } = {}) {
  return swiftFiles(resolve(root, 'host/apple/Sources')).flatMap((file) => linesOutside(readFileSync(file, 'utf8'), ['EXACT_INSPECT'])
    .filter(({ code }) => names.test(strings ? code : code.replace(/"[^"]*"/g, '')))
    .map(({ line, code }) => `${file.slice(root.length + 1)}:${line}: ${code.trim()}`));
}

// The agent's private calls: an event made in a window (CoreGraphics' setter
// outside its headers, an undocumented field), a gesture event's raw type and
// fields, Core Animation's filter keys.
const agentPrivate = /CGEventSetWindowLocation|CGEventField\(rawValue|CGEventType\(rawValue|inputRadius|"captureOnly"/;

test('every agent private call is inside an #if on EXACT_INSPECT', () => {
  expect(outsideInspect(agentPrivate, { strings: true })).toEqual([]);
});

// Undocumented defaults keys, notifications and string tables, which shipped
// code does not read (LLP 1035.003 §3).
const undocumented = /"Apple(InterfaceStyle|InterfaceThemeChangedNotification|ICUForce24HourTime|ICUForce12HourTime|Persistence)"|table: "Placeholders"|"WKJavaScriptException/;

test('shipped code reads no undocumented defaults key, notification or string table', () => {
  const read = [resolve(root, 'host/apple/Sources'), resolve(root, 'host/apple/webarm')].flatMap(swiftFiles).flatMap((file) =>
    readFileSync(file, 'utf8').split('\n').map((line, i) => [line, i]).filter(([line]) => !line.trim().startsWith('//') && undocumented.test(line))
      .map(([line, i]) => `${file.slice(root.length + 1)}:${i + 1}: ${line.trim()}`));
  expect(read).toEqual([]);
});

// Private classes, which shipped code does not name (LLP 1035.003). Shipped
// Swift is every file under host/apple but the tests, the test host, the probe
// and the touch runner, less the agent's files a production build leaves out
// (`agentSources` in Package.swift) and the lines under an `#if` only a
// development build satisfies. A line there names a class when it looks one up
// by name, other than the public classes listed; makes a class's name a string,
// other than in a statement listed as reporting it; writes a class into text
// and compares on the same line; or holds a literal shaped as a private UIKit,
// AppKit or Core Animation name. The check reads single lines, so a name kept
// and compared on a later line, or spelled another way, is beyond it.
const notShipped = /^host\/apple\/(tests|testhost|feel|fixtures)\/|^host\/apple\/touches\.swift$/;
const agentFiles = new Set([...readFileSync(resolve(root, 'host/apple/Package.swift'), 'utf8')
  .match(/let agentSources = [^[]*\[([\s\S]*?)\] : \[\]/)[1].matchAll(/"([^"]+)"/g)].map((m) => `host/apple/Sources/ExactKit/${m[1]}`));
const development = ['EXACT_INSPECT', 'EXACT_AGENT'];
const shippedLines = () => swiftFiles(resolve(root, 'host/apple')).map((file) => file.slice(root.length + 1))
  .filter((path) => !notShipped.test(path) && !agentFiles.has(path))
  .flatMap((path) => linesOutside(readFileSync(resolve(root, path), 'utf8'), development).map(({ line, code }) => ({ at: `${path}:${line}`, code })));
const publicLookup = /NSClassFromString\s*\(\s*"(UIViewReservedRegionKind|UIHingeInteraction|GCMouse|XCTestCase|ExactIntentsHooks)"\s*\)/g;
const lookup = /\b(NSClassFromString|objc_getClass|objc_getMetaClass|objc_lookUpClass|objc_getRequiredClass)\b/;
const namesClass = /\b(NSStringFromClass|object_getClassName|class_getName)\b|\.className\b/;
const classOf = /\btype\s*\(\s*of\s*:|\bobject_getClass\b|\.(classForCoder|superclass|subjectType)\b/;
const describes = /\.(description|debugDescription)\b|\bString\s*\(\s*(describing|reflecting)\s*:/;
const interpolatesClass = /\\#*\(([^()]|\([^()]*\))*?(\btype\s*\(\s*of\s*:|\bobject_getClass\b|\.(classForCoder|superclass|subjectType)\b)/;
const compares = /==|!=|~=|\b(switch|case)\b|\.\w*(contain|prefix|suffix|match|compar|range|starts|ends|equal)\w*\s*\(/i;
const privateLiteral = /"_(UI|NS|CA)[A-Z][A-Za-z]/;
// The statements that make a class's name a string only to report it: a
// hatch's undeclared views and recognizers, the log's first responder, the
// app's own delegate
const reports = new Set([
  'observed["class"] = String(describing: type(of: view))',
  'observed["class"] = String(describing: type(of: recognizer))',
  'let name = String(describing: type(of: recognizer))',
  'return String(describing: type(of: held))',
  'UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(AppDelegate.self))',
]);
const namesAClass = (code) => {
  const rest = code.replace(publicLookup, '');
  const asString = namesClass.test(rest) || (classOf.test(rest) && describes.test(rest));
  return lookup.test(rest) || privateLiteral.test(rest) || (asString && !reports.has(code.trim()))
    || ((asString || interpolatesClass.test(rest)) && compares.test(rest));
};

test('shipped code names no private class', () => {
  expect(agentFiles.size).toBeGreaterThan(0);
  const lines = shippedLines();
  expect(lines.some(({ at }) => at.startsWith('host/apple/videoarm/'))).toBe(true);
  expect(lines.filter(({ code }) => namesAClass(code)).map(({ at, code }) => `${at}: ${code.trim()}`)).toEqual([]);
});

test('the shipped-code check sees what it refuses', () => {
  for (const code of ['NSClassFromString("UISegment")', 'let resolve = NSClassFromString', 'objc_getClass("UISegment")',
    'String(describing: type(of: v)).contains("PIPPanel")', 'let name = NSStringFromClass(type(of: layer))', 'if v.className == "X"',
    'type(of: $0).description().hasPrefix("PIP")', 'if "\\(type(of: v))".contains("PIPPanel") {', 'let id = "_NSMenuItemAutoFillIdentifier"',
    'let name = String(describing: type(of: recognizer)); let hit = NSStringFromClass(type(of: v)) == "X"']) {
    expect(namesAClass(code)).toBe(true);
  }
  for (const code of ['NSClassFromString("GCMouse")', 'let name = String(describing: type(of: recognizer))', 'log("hatch: delegate \\(type(of: delegate))")',
    'case let d as Double where type(of: value) == Double.self: return "\\(d)"']) {
    expect(namesAClass(code)).toBe(false);
  }
  // Only a branch no production build compiles is left out
  const fixture = [
    '#if !(EXACT_INSPECT)', 'let a = NSClassFromString("A")', '#endif',
    '#if EXACT_INSPECT || os(iOS)', 'let b = NSClassFromString("B")', '#endif',
    '#if os(iOS) && (EXACT_AGENT)', 'let c = NSClassFromString("C")', '#elseif !EXACT_INSPECT', 'let d = NSClassFromString("D")', '#else', 'let e = NSClassFromString("E")', '#endif',
    '#if !(os(iOS) && EXACT_INSPECT)', 'let f = NSClassFromString("F")', '#endif',
    '#if EXACT_INSPECT', '#if os(macOS)', 'let g = NSClassFromString("G")', '#endif', '#endif',
    '#if os(macOS) // && EXACT_INSPECT', 'let h = NSClassFromString("H")', '#endif',
    '#if EXACT_INSPECT /* development */ || os(macOS)', 'let i = NSClassFromString("I")', '#endif',
  ].join('\n');
  const kept = linesOutside(fixture, development).filter(({ code }) => namesAClass(code)).map(({ code }) => code.match(/let (\w)/)[1]);
  expect(kept).toEqual(['a', 'b', 'd', 'e', 'f', 'h', 'i']);
});

test('every Swift line naming inspect mode is inside an #if on EXACT_INSPECT', () => {
  expect(outsideInspect(inspectNames)).toEqual([]);
});

const bake = resolve(root, 'target/clients');
const productionBinaries = () => !existsSync(bake) ? [] : readdirSync(bake).map((d) => join(bake, d, 'com.exact.theme-fixture/macos/aarch64-apple-darwin/embedded/production/standalone/Theme fixture')).filter(existsSync);

test.skipIf(productionBinaries().length === 0)('a production Mac bake has none of inspect mode\'s strings or symbols', () => {
  for (const binary of productionBinaries()) {
    const strings = spawnSync('strings', [binary], { encoding: 'utf8', maxBuffer: 1 << 28 }).stdout;
    expect(strings).not.toContain('not a view of the app');
    expect(strings).not.toContain('identity alone is larger than a report');
    for (const word of ['CGEventSetWindowLocation', 'inputRadius', 'captureOnly', 'AppleInterfaceStyle', 'AppleInterfaceThemeChangedNotification', 'AppleICUForce24HourTime', 'AppleICUForce12HourTime', 'ApplePersistence']) expect(strings).not.toContain(word);
    const symbols = spawnSync('nm', [binary], { encoding: 'utf8', maxBuffer: 1 << 28 }).stdout;
    expect(symbols.split('\n').filter((s) => /InspectHost|InspectOverlay|7Inspect/.test(s))).toEqual([]);
  }
});
