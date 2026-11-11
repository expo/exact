// What the app's Swift module dylib (libexact_modules.dylib) is generated and compiled from, as one digest.
//
// A development archive stamps it in its receipt (build.nativeExtras); a repack recomputes it after its bake and
// refuses when it differs. The Contract and TypeScript reach this dylib without changing compat.json: a `use` or an
// import of a package with an Apple implementation (even one of the SDK's own @exact/* packages, which no package.json
// lists) composes that package's Swift into it (ExactComposed), and when the app composes at all, app.json's data words
// (ExactDataKeys) and app.tamber's token names (tamber.swift) are compiled into it as well.
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { basename, relative } from 'node:path';
import { appleImplementations } from '../../js/bake/src/typescript.mjs';

const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const canonical = (value) => Array.isArray(value) ? `[${value.map(canonical).join(',')}]` : value && typeof value === 'object' ? `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}` : JSON.stringify(value ?? null);

export function nativeExtras(app, platform = 'ios') {
  // Paths say where a checkout is, not what it holds: every file is named by where it is in its package or app,
  // and by its bytes.
  const named = (value) => typeof value === 'string' && value.startsWith('/') ? (existsSync(value) ? `${basename(value)}:${sha(readFileSync(value))}` : `missing:${basename(value)}`) : Array.isArray(value) ? value.map(named) : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([k, v]) => [k, named(v)])) : value;
  const packages = appleImplementations(app.dir, platform).map(named);
  const modules = app.modulesFor(platform);
  const own = (modules.apple ?? []).map(file => [relative(app.dir, file), sha(readFileSync(file))]);
  const frameworks = (modules.frameworks ?? []).map(file => relative(app.dir, file));
  const composing = packages.length > 0 || own.length > 0;
  const tamber = composing && existsSync(`${app.dir}/app.tamber`) ? sha(readFileSync(`${app.dir}/app.tamber`)) : null;
  return sha(canonical({ version: 1, packages, own, frameworks, composing, tamber, data: composing ? app.manifest.data ?? null : null, tags: modules.tags ?? [] }));
}
