// What the app's Swift module dylib (libexact_modules.dylib) is generated and compiled from, as one digest.
//
// A development archive stamps it in its receipt (build.nativeExtras); a repack recomputes it after its bake and
// refuses when it differs. The Contract and TypeScript reach this dylib without changing compat.json: a `use` or an
// import of a package with an Apple implementation (even one of the SDK's own @exact/* packages, which no package.json
// lists) composes that package's Swift into it (ExactComposed), and when the app composes at all, app.json's data words
// (ExactDataKeys) and app.tamber's token names (tamber.swift) are compiled into it as well.
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { basename, relative, resolve } from 'node:path';
import { appleImplementations } from '../../js/bake/src/typescript.mjs';
import { nativePackageConfig } from '../../js/bake/src/package-config.mjs';
import { swiftPackageInventories } from '../../scripts/package-libraries.mjs';
import { experimental } from './toolchain.mjs';
import { fitLinkedSections, reserveLinkedPlan } from './sections.mjs';
import { widgetsDigest } from './widgets.mjs';

/** Experimental (`EXACT_EXPERIMENTAL=1`): whether an iPhone development archive is one repack.mjs can rebuild from a
 * later bake. Its executable links the plan's sections with room to spare (an app with widgets links its plan into its
 * own ExactCore, which reserves room already), and its receipt records repackReceipt. Never a distributed build, nor
 * one that bakes production trust. */
export const repackBase = ({ ios, tv, device, ipa, developmentArchive, distribution = false }, env = process.env) =>
  !!(ios && !tv && device && ipa && developmentArchive && !distribution && env.EXACT_UPDATE_TRUST !== 'production' && experimental(env));
/** The linked plan directory a repack base links from: its sections reserved (sections.mjs). */
export const reservedPlan = (dir, base) => dir && base ? reserveLinkedPlan(dir) : dir;
/** A repack base's plan, fitted into its reserved sections after the link. */
export const fitReservedPlan = (executable, dir) => fitLinkedSections(executable, Object.fromEntries([['plan', 'app.plan'], ['compat', 'compat.json'], ['bytecode', 'app.hbc']].map(([section, file]) => [section, readFileSync(resolve(dir, file))])));
/** What a repack must find unchanged in its base's receipt: the Swift module's inputs and, for an app with widgets,
 * what the extension's binary is generated from (widgetsDigest, over the packaged extension kinds). */
export const repackReceipt = (app, graph, widgetKinds = null) => ({ nativeExtras: nativeExtras(app, 'ios', graph.extensions, graph.contractSources), ...(widgetKinds ? { widgets: widgetsDigest(app, widgetKinds) } : {}) });

const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const canonical = (value) => Array.isArray(value) ? `[${value.map(canonical).join(',')}]` : value && typeof value === 'object' ? `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}` : JSON.stringify(value ?? null);

export function nativeExtras(app, platform = 'ios', extensions, contractSources = []) {
  // Paths say where a checkout is, not what it holds: every file is named by where it is in its package or app,
  // and by its bytes.
  const named = (value) => typeof value === 'string' && value.startsWith('/') ? (existsSync(value) ? `${basename(value)}:${sha(readFileSync(value))}` : `missing:${basename(value)}`) : Array.isArray(value) ? value.map(named) : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([k, v]) => [k, named(v)])) : value;
  // An implementation's package root is a directory, which has no bytes to name: a Swift package it ships by path is
  // named by its files instead, as the build's inputs name it (swiftPackageInventories).
  const implementations = appleImplementations(app.dir, platform, extensions, contractSources);
  const packages = implementations.map(({ swiftPackageRoot, ...impl }) => named(impl));
  const shipped = swiftPackageInventories(app, implementations, platform).map(row => row.identity);
  const modules = app.modulesFor(platform);
  const own = (modules.apple ?? []).map(file => [relative(app.dir, file), sha(readFileSync(file))]);
  const frameworks = (modules.frameworks ?? []).map(file => relative(app.dir, file));
  const composing = packages.length > 0 || own.length > 0;
  const tamber = composing && existsSync(`${app.dir}/app.tamber`) ? sha(readFileSync(`${app.dir}/app.tamber`)) : null;
  // The packages' configuration, which the composed module carries (compose.mjs `composedModule`).
  const config = composing ? nativePackageConfig(app.manifest, platform) : {};
  return sha(canonical({ version: 1, packages, own, frameworks, composing, tamber, data: composing ? app.manifest.data ?? null : null, tags: modules.tags ?? [], ...(Object.keys(config).length ? { config } : {}), ...(shipped.length ? { shipped } : {}) }));
}
