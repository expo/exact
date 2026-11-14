// An app's commands (LLP 1086; LLP 1116.006 D7): what `bun exact.mjs <verb>` runs in an app that
// `exact new` made. Its exact.mjs is a few lines that find this checkout (EXACT) and call `run`, so
// a builder reading it learns where the verbs are (`bun exact.mjs --help`), not this code. An
// exact.mjs written before this file held all of it, and still runs; `bun exact.mjs update` thins it.
import { spawn, spawnSync } from 'node:child_process';
import { appendFileSync, existsSync, mkdirSync, readFileSync, realpathSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { constants, tmpdir } from 'node:os';
import { basename, dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { noteBunLoaded } from '../js/bake/src/__exact_env.mjs';

/** The hosts a `test`, `agent` or `verify` may name first. */
const APP_HOSTS = ['web', 'ios', 'macos', 'mac', 'linux', 'windows', 'android'];
/** app.json's text as JSON, a leading byte-order mark named with its fix (scripts/app.mjs `parseManifest`; this
 * module keeps its own copy, since copied SDKs carry it alone). */
export function parseManifest(text) {
  if (text.charCodeAt(0) === 0xfeff) throw new Error('starts with a byte-order mark (U+FEFF), which JSON does not allow; save it as UTF-8 without a BOM');
  return JSON.parse(text);
}
/** The host `verb` runs on, taken off `rest` when its first word names one (help's optional `[web|ios|…]`): a test
 *  file or an op written first stays where it is, and the run is on a host the app ships (`shippedHost`; null when
 *  none drives here). */
export const hostOf = (verb, rest, manifest = {}, os = process.platform) => ['test', 'agent', 'verify'].includes(verb) && APP_HOSTS.includes(rest[0]) ? rest.shift()
  : ['test', 'agent', 'verify'].includes(verb) ? shippedHost(verb, manifest, os).host : 'web';

/** The platforms other than the web that a verb drives, each with whether it drives on `os` and, when not, why:
 *  `verify` has legs for three; `test` and `agent` drive every host the driver carries (scripts/agent.mjs). */
const LEGS = {
  ios: (os) => os === 'darwin' || 'its simulator needs macOS',
  macos: (os, verb) => verb === 'verify' ? 'verify has no leg for it' : os === 'darwin' || 'it needs macOS',
  windows: (os) => os === 'win32' || 'it runs on Windows',
  // The driver opens the Linux host's binary as a Unix one (app.mjs `linuxBinary`): on macOS and Linux.
  linux: (os, verb) => verb === 'verify' ? 'verify has no leg for it' : ['darwin', 'linux'].includes(os) || 'its headless host is driven on macOS and Linux',
  android: () => true,
};

/** The host a `test`, `agent` or `verify` that names none runs on: the web when the manifest names it (`host.web`, a
 *  web store or binary) or names no platform at all, as an app `exact new` made does; else the first platform the
 *  manifest names (under `host`, `deploy.store` or `deploy.binaries`, as the build and deploy read them) that the verb
 *  drives on `os`, this machine's own first. With none, `host` is null and `why` says so in a line: a web run would
 *  check a host the app does not ship. */
export function shippedHost(verb, manifest, os = process.platform) {
  const named = new Set([manifest?.host, manifest?.deploy?.store, manifest?.deploy?.binaries].flatMap((table) => Object.keys(table ?? {})));
  if (!named.size || named.has('web')) return { host: 'web' };
  const own = { darwin: ['ios', 'macos'], win32: ['windows'], linux: ['linux'] }[os] ?? [];
  const order = [...new Set([...own, 'ios', 'macos', 'windows', 'linux', 'android'])];
  const leg = (p) => p in LEGS ? LEGS[p](os, verb) : `${verb} has no host for it`;
  const host = order.find((p) => named.has(p) && leg(p) === true);
  if (host) return { host };
  const reasons = [...named].map((p) => `${p}: ${leg(p)}`).join('; ');
  const then = verb === 'verify' ? 'so only the semantics check runs' : `so ${verb} runs nowhere unless named (\`${verb} web\` runs it on the web anyway)`;
  return { host: null, why: `app.json names ${[...named].join(', ')} and not the web, and none drives here (${reasons}), ${then}` };
}

/** The compiler's warnings in what a build said (LLP 1116.004 D7), each once as `contract build` prints
 * it, `file:line:col warning [id] text`: the web compiler's or a bake's own line, Cargo's for a build
 * script's (`warning: <crate>@<version>: …`) or a linked bake's (`warning: <app>: …`, linked-plan.mjs); its
 * file from `cwd` when it is below it. */
export function compilerWarnings(text, cwd = process.cwd()) {
  const lines = new Set();
  for (const line of String(text ?? '').split('\n')) {
    const m = /^(?:warning: [^\s:]+: )?(\S(?:.*?\S)?):(\d+:\d+ warning \[[\w-]+\] .*)$/.exec(line);
    if (!m) continue;
    const near = relative(cwd, m[1]);
    lines.add(`${isAbsolute(m[1]) && !near.startsWith('..') ? near : m[1]}:${m[2]}`);
  }
  return [...lines];
}

/** What a failed JS-target web build (`host/web-js/build.mjs`) said that is the app's to act on, from its stderr: the
 * types app.ts imports when rewritten, warnings, the diagnostics, then the compiler's warnings. Cargo's progress, a
 * rustc warning's excerpt and Bun's trace (a frame and its version line) are left out; with no diagnostic, the last
 * three lines stand for one. */
export function webBuildFailure(stderr) {
  const ambientDiagnostic = (l) => /^.+:\d+:\d+: .+ is unavailable in data sources[;:]/.test(l);
  const all = (stderr ?? '').trim().split('\n').filter((l) => ambientDiagnostic(l) || !/^\s*(Compiling|Finished|Running)/.test(l) && !compilerWarnings(l).length);
  const warnings = all.filter((l) => !ambientDiagnostic(l) && /^warning: /.test(l)), lines = all.filter((l) => ambientDiagnostic(l) || !/^\s*warning/.test(l));
  // Keep TypeScript's lines (`app.ts(2,8): error TS…`), the shared ambient
  // check's file:line:col diagnostics, including paths with spaces, and its
  // refusals of package code (js/bake/src/typescript.mjs `capturedSources`).
  const packageRefusal = (l) => / (?:requires peer|imports Node's built-in|uses the DOM's|uses Node's|loads a module by a computed name|loads code from a string|uses the page's)|, (?:which is not installed|a native addon|which the checked capture did not resolve)|: not an npm package name and version| must be a constant the build reads without running|^(?:Error: )?(?:package declarations: )?two copies of |^(?:Error: )?package (?:declarations exceed|code exceeds) 64 MiB/.test(l);
  const marker = /^(error|[A-Z]\w*Error|E[A-Z]+)\b:?/;
  const kept = (l) => marker.test(l.trim()) || /\): error TS\d+:|^(?:Error: )?(?:tsconfig: |module outside captured app: |source links are not captured: )|^note: /.test(l) || ambientDiagnostic(l) || packageRefusal(l) || /\bunoptimized$|\bnot on PATH\b|import\.meta\.env/.test(l);
  const message = [];
  for (let i = 0; i < lines.length; i++) {
    if (!kept(lines[i])) continue;
    message.push(lines[i]);
    // A marker with nothing after it says its message below, to the next diagnostic or Bun's trace: wasm-bindgen's
    // schema mismatch is `error: `, a blank line, then paragraphs.
    if (!/^\s*(error|[A-Z]\w*Error|E[A-Z]+)\b:\s*$/.test(lines[i])) continue;
    let end = i + 1;
    while (end < lines.length && !kept(lines[end]) && !/^\s+at |^Bun v\d/.test(lines[end])) end++;
    while (end > i + 1 && !lines[end - 1].trim()) end--;
    message.push(...lines.slice(i + 1, end));
    i = end - 1;
  }
  // The bundler's refusal, in the words the type check uses (`Error: ` is its wrapper).
  const reason = (message.length ? message : lines.slice(-3)).map((l) => l.replace(/^Error: (?=module outside captured app: )/, '')).join('\n');
  const types = all.filter((l) => /^app\.contract\.d\.ts: /.test(l));
  return [...types, ...warnings, reason, ...compilerWarnings(stderr)];
}

/** A drive's lines for the compiler's `warnings` (`compilerWarnings`) of the app in `dir` built for `host`: the
 * warnings no build of it said before, each in full, then one line counting them all and naming the file that lists
 * them, `.exact/warnings-<host>.txt`, which this replaces. A drive repeated over unchanged warnings says the count
 * alone; one with none says nothing. */
export function warningLines(dir, host, warnings) {
  const file = resolve(dir, '.exact', `warnings-${host}.txt`), shown = relative(process.cwd(), file);
  let before = null; try { before = new Set(readFileSync(file, 'utf8').split('\n').filter(Boolean)); } catch { /* none said yet */ }
  // Replaced whole, so a drive beside this one reads either list, never half of one.
  try { mkdirSync(dirname(file), { recursive: true }); writeFileSync(`${file}.${process.pid}.tmp`, warnings.map((w) => `${w}\n`).join('')); renameSync(`${file}.${process.pid}.tmp`, file); } catch { /* said in full below */ }
  if (!warnings.length) return [];
  const fresh = before ? warnings.filter((w) => !before.has(w)) : warnings;
  const where = shown.startsWith('..') ? file : shown;
  const noted = warnings.filter(information).length, counted = warnings.length - noted;
  const count = `compiler: ${[(counted || !noted) && `${counted} warning${counted === 1 ? '' : 's'}`, noted && `${noted} note${noted === 1 ? '' : 's'} (not counted)`].filter(Boolean).join(' and ')}`;
  if (!fresh.length) return [`${count}, as before (${where} lists them; bun exact.mjs contract build quotes each line)`];
  // The colours' one line is the whole set's, said when any of its lines is new (a changed colour at the same site).
  const colour = (w) => / warning \[lower-colour-one-scheme\] /.test(w), anyColour = fresh.some(colour);
  const said = oneScheme(warnings.filter((w) => !webOnly(w))).filter((w) => fresh.includes(w) || (colour(w) && anyColour && !warnings.includes(w)));
  // In the compiler's order (`contract::warning_rank`), the symbols only the web draws blank one line: 21 of them hid
  // the euxy port's header warning.
  const web = fresh.filter(webOnly), one = web.length === 1;
  const group = web.length ? [`${web[0].split(' warning [')[0]} warning [lower-symbol-platform] ${web.length} SF Symbol use${one ? '' : 's'} draw${one ? 's' : ''} blank on the web, here first: an icon token or a role draws on every host (${where} lists each, and how)`] : [];
  return [...said, ...group, `${count}${fresh.length < warnings.length ? `, ${fresh.length} new (above)` : ''}; ${where} lists them all`];
}

/** A compiler warning that is information, not a problem: `tamber-unreachable` names a control whose host draws the
 *  platform's own look, which the author should know of and cannot change (LLP 1121 D9, amended 2026-10-30). A drive
 *  and verify say it but do not count it. contract's `warning_rank::information` is the same test. */
export const information = (w) => / warning \[(tamber-unreachable|style-undrawn)\] /.test(w);

/** The colours that stay one in dark mode (`lower-colour-one-scheme`), past one, as one line: their closing line with
 *  the first three sites' `file:line`, theme tokens included, at the first's place (LLP 1168.000 D7.2: a chat app said
 *  31 in every build). `contract build --json` lists each, with a token's own repair. contract_lower's
 *  `warning_rank::one_scheme_line` is the same. */
export function oneScheme(warnings) {
  const said = (w) => /warning \[lower-colour-one-scheme\] /.test(w), closing = (w) => /warning \[lower-colour-one-scheme\] these /.test(w);
  const close = warnings.find(closing), sites = warnings.filter((w) => said(w) && !closing(w));
  if (!close || sites.length < 2) return warnings;
  const place = (w) => w.split(' warning [')[0], line = (w) => place(w).replace(/:\d+$/, '');
  const shown = [...sites.slice(0, 3).map(line), ...(sites.length > 3 ? ['…'] : [])].join(', ');
  let message = close.split('] ').slice(1).join('] ').replace(/^(these \d+ (?:\S+ )*?(?:colours|tokens)) /, `$1 (${shown}) `)
    .replace('each line above says how to repair it', '`contract build --json` says how to repair each')
    .replace(" (each token's line says its own repair)", " (`contract build --json` says each token's repair)");
  if (!message.includes('`contract build --json`')) message += ' `contract build --json` names each.';
  const one = `${place(sites[0])} warning [lower-colour-one-scheme] ${message}`;
  return warnings.flatMap((w) => (w === sites[0] ? [one] : w === close || sites.includes(w) ? [] : [w]));
}

/** A compiler warning only the web's picture shows: an SF Symbol drawn blank where the web is the one target besides
 *  Apple's (contract's `web_only`). Its first use names the targets, `ships web (…), where it is blank`; a later one,
 *  `blank on web too`. */
export const webOnly = (w) => /warning \[lower-symbol-platform\] /.test(w) && (/ blank on web too/.test(w) || /ships web \((?:(?!\) and ).)*?, where it is blank/.test(w));

/** The window a Windows app opens in, `[width, height]`: app.json's `host.windows.window`, else the host's default
 * (exact_windows::Window). A drive of an app on Windows is that size, as a person first sees it. */
export const windowsWindow = (manifest) => [manifest?.host?.windows?.window?.width ?? 1280, manifest?.host?.windows?.window?.height ?? 720];

/** exact2's verbs for the app `name` in `dir`: each the script it runs, from the checkout, and its
 * first arguments. `host` is a drive's (`test ios`); a game has its own and no `verify` or `pattern`. */
export function appVerbs(name, dir, { host = 'web', game = false, os = process.platform } = {}) {
  return {
    web: ['host/web/dev.mjs', '--app', name],
    'web-build': ['host/web/build.mjs', name],
    serve: ['host/web/serve.mjs'],
    test: ['scripts/agent.mjs', host, '--app', name],
    agent: ['scripts/agent.mjs', host, '--app', name],
    // Builds itself, so not below (LLP 1116.004 D7): one report of the build, the tests and check.
    ...game ? {} : { verify: ['scripts/verify.mjs', ...(host ? [host] : []), '--app', name] },
    ios: ['host/apple/build.mjs', '--ios', `${name}-apple`, ...(os === 'linux' ? ['--no-install'] : [])],
    mac: ['host/apple/build.mjs', `${name}-apple`],
    // An iPhone build, uploaded to AppDrop: the link a person opens to install it.
    share: ['scripts/share.mjs', '--app', name],
    linux: ['scripts/build-linux.mjs', name],
    windows: ['host/windows/build.mjs', name],
    // The APK on the Canvas reader, installed (LLP 1107 D1); `--headless` the Linux host for Android instead.
    android: ['scripts/agent-android.mjs', 'build', name, '--apk', '--install'],
    update: ['scripts/exact.mjs', 'new', dir, '--update'],
    lock: ['scripts/app-cli.mjs', '--lock', dir],
    add: ['scripts/exact.mjs', 'add'],
    contract: ['scripts/exact.mjs', 'contract'],
    tamber: ['scripts/exact.mjs', 'tamber'],
    hatch: ['scripts/exact.mjs', 'hatch'],
    snapback: ['scripts/exact.mjs', 'snapback', dir],
    backend: ['scripts/exact.mjs', 'backend', dir],
    ...game ? {} : { pattern: ['scripts/exact.mjs', 'pattern'] },
    docs: ['scripts/exact.mjs', 'docs'],
    feedback: ['scripts/feedback.mjs'],
    // A game's own: its hostless Rust tests, its proof's baseline, and the rows its dev loop and proof write
    // (game/status.mjs; exact2's game/README.md). An app's dev loop listens only once it has built, so it writes none.
    ...game ? {
      status: ['scripts/exact.mjs', 'status', dir],
      check: ['game/app/shells.mjs', dir, '--check'],
      game: ['scripts/exact.mjs', 'game'],
      'test-rust': ['game/app/shells.mjs', dir, '--test'],
      shells: ['game/app/shells.mjs', dir],
      lock: ['game/app/shells.mjs', dir, '--lock'],
      prove: ['game/prove.mjs', dir],
    } : {},
  };
}

/** Whether each of `appVerbs`' verbs runs app.json's `before_build` first: those that build, run, compile or check the
 * app, and so read what it makes (a fetched asset, a generated source). A verb whose arguments decide is a test of
 * them; `test` and `agent` prepare in `run`, where they build first. */
export const beforeBuild = {
  status: false, web: true, 'web-build': true, serve: true, test: false, agent: false, verify: true, ios: true, mac: true, share: true,
  linux: true, windows: true, android: true, update: false, lock: false, hatch: false, snapback: true, backend: false, docs: false,
  feedback: false, add: false,
  contract: ([command, ...rest]) => ['build', 'types', 'rust', 'verify', 'test', 'lean', 'symbols', 'gallery'].includes(command)
    || command === 'tamber' && checksDesign(rest),
  tamber: (rest) => checksDesign(rest),
  pattern: (rest) => rest.some((arg) => !arg.startsWith('-')),
  // A game's own: its compile, Rust tests and proof prepare; `shells` writes .shells/ without a build
  check: true, game: false, 'test-rust': true, shells: false, prove: true,
};

/** The Tamber commands that compile the app's design: its check, evaluation, IR and generated code, the tokens it
 * imports from CSS, which it checks, and the design it serves (`tool` without `--via`, which reaches the app's dev loop
 * instead). */
const checksDesign = ([command, ...rest]) => ['check', 'eval', 'ir', 'gen', 'serve'].includes(command)
  || command === 'import' && rest[0] === 'css' || command === 'tool' && !rest.includes('--via');

/** `bun exact.mjs <verb> [arguments]` for the app `name` in `dir`, with the checkout `exact2`; the exit code.
 * A build verb under `--json` (help.mjs `jsonBuilt`) prints exactly one object on stdout however it ends: the artifact
 * the build named, or `{code, error}` with a non-zero status, a refusal or a throw included. */
export async function run(options) {
  const { args } = options;
  if (!JSON_BUILDS.includes(args[0]) || !args.includes('--json') || args.includes('--help') || args.includes('-h')) return verbRun(options, null);
  let said = false;
  const reply = (object) => { said = true; console.log(JSON.stringify(object)); };
  let status;
  try { status = await verbRun(options, reply); }
  catch (error) {
    const message = String(error?.message ?? error);
    console.error(message);
    if (!said) reply({ code: 'build_failed', error: message });
    return 1;
  }
  if (!said) reply({ code: 'build_failed', error: `${args[0]} ${status ? `failed (exit ${status})` : 'named no artifact'}; what it said is on stderr` });
  return said && !status ? 0 : status || 1;
}

async function verbRun({ dir, name, game = false, exact2, args }, reply) {
  // Before any process starts: which settings Bun loaded from this directory's `.env` files (LLP 1153 D4).
  noteBunLoaded();
  // The design tool's tests, drives and verify run on this checkout's own port (port-lock.mjs `checkoutPort`, unless
  // EXACT_TOOL_PORT names one), from a copy staged with that origin and its stub (tamber-tool-port.mjs
  // `stageToolTests`), so its stub never binds the 7740 a person's tool holds; EXACT_TOOL_PORT=7740 runs them in place.
  if (['test', 'agent', 'verify'].includes(args[0]) && !args.includes('--help') && !args.includes('-h') && existsSync(resolve(exact2, 'apps/design-tool')) && realpathSync(dir) === realpathSync(resolve(exact2, 'apps/design-tool'))) {
    process.env.EXACT_TOOL_PORT ??= String((await import(resolve(exact2, 'scripts/port-lock.mjs'))).checkoutPort(resolve(exact2)));
    const tool = await import(resolve(exact2, 'scripts/tamber-tool-port.mjs')), rest = args.slice(1);
    const tested = tool.testFiles(args[0], rest, (await import(resolve(exact2, 'scripts/help.mjs'))).knownFlags(args[0]));
    const staged = tool.stageToolTests(dir, rest, i => tested.has(i));
    if (staged) { dir = staged.dir; args = [args[0], ...staged.args]; }
  }
  const [verb, ...rest] = args;
  // The app's own verbs (paint, minesweeper, ledger: a regenerated file dropped the ones added to it): app.json's
  // `commands`, each a command as argv (`"seed": ["bun", "seed.mjs"]`), run in the app's directory with the
  // arguments after the verb. A game's app.json is optional.
  const manifest = resolve(dir, 'app.json');
  let declared = {};
  try { if (existsSync(manifest)) declared = parseManifest(readFileSync(manifest, 'utf8')); }
  catch (error) {
    const message = `${manifest}: ${error.message}`;
    if (!reply) throw new Error(message);
    console.error(message);
    reply({ code: 'app_json', error: message });
    return 1;
  }
  // A game's hosts are its bake's (game/README.md): its drives stay the web's unless named.
  const host = hostOf(verb, rest, game ? {} : declared);
  const verbs = appVerbs(name, dir, { host, game });
  const own = declared.commands ?? {};
  const clash = Object.keys(own).find((name) => verbs[name]);
  if (clash) {
    const message = `app.json commands.${clash}: ${clash} is one of exact.mjs's own verbs; give the app's another name`;
    console.error(message);
    reply?.({ code: 'app_json', error: message });
    return 2;
  }
  // Help (LLP 1116 D4): `--help` or `-h` among a verb's arguments, `help [<verb>]` or no verb prints exact2's own
  // text (its scripts/help.mjs) and starts nothing. An app's own verb gets its arguments as given. `all` anywhere
  // among them asks for a verb's whole text (`web --help all`, `help web all`, `help all web`); with no verb it is
  // the index (`--help all`, `help all`), never a verb (LLP 1116.010 D2).
  const known = (a) => verbs[a] || own[a];
  const asked = !verb || ['help', '--help', '-h'].includes(verb) ? rest.find(known) ?? rest.find((a) => !a.startsWith('-') && a !== 'all') ?? '' : verbs[verb] && (rest.includes('--help') || rest.includes('-h')) ? verb : null;
  if (asked !== null) {
    const help = await import(resolve(exact2, 'scripts/help.mjs')).catch(() => null);
    if (!help) { console.error(`no Exact checkout at ${exact2}: EXACT names the checkout`); return 1; }
    return help.printHelp(asked, { verbs: Object.keys(verbs), own, all: rest.includes('all') });
  }
  if (verb === 'web' && rest.includes('--ready') && !game) {
    console.error('web --ready waits for a game\'s dev loop, which serves while it builds; an app\'s web prints its URL once it serves');
    return 2;
  }
  if (verb === 'status' && game || verb === 'web' && rest.includes('--ready')) {
    const {readStatus, formatStatus, waitReady} = await import(resolve(exact2, 'game/status.mjs'));
    if (verb === 'status') { console.log(formatStatus(readStatus(dir))); return 0; }
    try { const row = await waitReady(dir, {timeout:Number(rest.includes('--timeout') ? rest[rest.indexOf('--timeout') + 1] : 600)}); console.log(`READY ${row.url}`); return 0; }
    catch (e) { console.error(e.message); return 1; }
  }
  if (!verbs[verb] && !own[verb]) {
    console.error(`Usage: bun exact.mjs <${[...Object.keys(verbs), ...Object.keys(own)].join('|')}> [arguments for that script]\nbun exact.mjs help says what each does.`);
    return 2;
  }
  // A failed, slow or finished command says how to write a feedback note (scripts/feedback-note.mjs; skipped in a
  // checkout without it).
  const feedback = verb === 'feedback' ? null : await import(resolve(exact2, 'scripts/feedback-note.mjs')).catch(() => null);
  let interrupted = false;
  let stopped = null; // the first signal passed on to a child
  const started = Date.now();
  // The diary's command log (exact2's docs/diary.md): which exact2, what ran, how it ended. No arguments.
  const log = (exit) => { if (verb !== 'feedback') try {
    const commit = spawnSync('git', ['-C', exact2, 'rev-parse', '--short=9', 'HEAD'], { encoding: 'utf8' }).stdout?.trim() || null;
    mkdirSync(resolve(dir, '.exact'), { recursive: true });
    appendFileSync(resolve(dir, '.exact/commands.jsonl'), JSON.stringify({ at: new Date(started).toISOString(), exact2: commit, verb: ['test', 'agent', 'verify'].includes(verb) ? `${verb} ${host ?? 'none'}` : verb, exit, ms: Date.now() - started }) + '\n');
  } catch {}
  if (feedback) try {
    for (const line of feedback.nudge(dir, { verb, host, exit, ms: Date.now() - started, args: rest, interrupted })) console.error(line);
  } catch {}
  return exit; };
  // A verb that builds the app bundles app.ts with rolldown from this checkout's node_modules (the web's build and
  // every native bake); without it Bun's auto-install spins silently instead of failing. A web drive of a site
  // already built (`--web-dist`, or a `--url` that is not a path: a path is the app's own, built first, as
  // `drivesWeb` below) and `serve` of a build build nothing; a native `--url` (live reload) still bakes.
  // (Read as `drivesWeb` reads it, after `--backend <url>` is taken out below.)
  const backend = rest.indexOf('--backend'), driven = backend < 0 ? rest : rest.filter((_, i) => i !== backend && i !== backend + 1);
  const builtSite = (verb === 'test' || verb === 'agent') && host === 'web'
    && driven.some((a, i) => (a === '--url' && !driven[i + 1]?.startsWith('/')) || a === '--web-dist');
  const builds = ['web', 'web-build', 'test', 'agent', 'verify', 'ios', 'mac', 'linux', 'windows', 'android'].includes(verb) && !builtSite;
  if (builds && !['rolldown', 'rolldown.exe'].some((bin) => existsSync(resolve(exact2, 'node_modules/.bin', bin)))) {
    const error = `exact's node_modules is missing: run \`bun install --frozen-lockfile\` in ${exact2} (every build's type check imports rolldown from it)`;
    console.error(`${verb}: ${error}`);
    // Under `--json` the one object on stdout says so too: a build's `{code, error}`, a drive's or verify's `{error, exit}`.
    if (rest.includes('--json')) { if (reply) reply({ code: 'node_modules_missing', error }); else console.log(JSON.stringify({ error, exit: 1 })); }
    return log(1);
  }
  // A drive with no host the app ships here runs nowhere, and says why (verify runs its semantics check alone).
  if (host === null && verb !== 'verify') {
    const { why } = shippedHost(verb, declared);
    console.error(`${verb}: ${why}`);
    reply?.({ code: 'platform_refused', error: why });
    return log(2);
  }
  // A native build that failed under a cargo that is not rustup's (Homebrew's `rust`, ahead of ~/.cargo/bin) says so:
  // that cargo ignores rust-toolchain.toml and builds with a rustc whose std has no iOS target (`cargoProxyFault`).
  const native = ['ios', 'mac', 'linux', 'windows', 'android', 'share'].includes(verb) || (['test', 'agent', 'verify'].includes(verb) && host !== 'web');
  const withCargoFault = async (exit) => {
    if (exit && !interrupted && native) try {
      const pin = Bun.TOML.parse(readFileSync(resolve(exact2, 'rust-toolchain.toml'), 'utf8')).toolchain.channel;
      const fault = (await import(resolve(exact2, 'scripts/exact.mjs'))).cargoProxyFault(pin);
      if (fault) console.error(`exact: ${fault}`);
    } catch {}
    return exit;
  };
  // A build verb's `--json` (`reply`): one object on stdout, and the build's output on stderr. A failure is
  // `{code, error}`, as share's, with a non-zero status. It reports a finished artifact, so it takes no launch, no build
  // left running and no test run; and a flag the build's script refuses is refused before anything is built.
  const jsonBuilt = reply != null;
  if (jsonBuilt && ['--run', '--prebuild', '--test'].some((flag) => rest.includes(flag))) {
    const error = `${verb} --json reports the built artifact, so it takes none of --run, --prebuild and --test`;
    console.error(error);
    reply({ code: 'json_flags', error });
    return log(2);
  }
  if (jsonBuilt) {
    let refusal = null;
    const refused = (await import(resolve(exact2, 'scripts/help.mjs'))).guardFlags(verb, rest, { fail: (message) => { refusal = message; console.error(message); }, exit: (code) => ({ code }) })?.code;
    if (refused != null) { reply({ code: 'unknown_flag', error: refusal }); return log(refused); }
  }
  if (['ios', 'mac'].includes(verb)) {
    const error = (await import(resolve(exact2, 'scripts/help.mjs'))).appleBuildRefusal(rest);
    if (error) { console.error(error); reply?.({ code: 'build_failed', error }); return log(2); }
  }
  // A native verb names its platform in app.json the first time, after that platform's setup rows, and keeps the
  // generated workspace current unless the app authored one (LLP 1153 D1.2; scripts/app-shells.mjs). A drive's
  // host counts (`test ios`). Absent from a checkout that lacks the module (the scaffold's test SDK), it is skipped.
  const platform = game || rest.includes('--help') || rest.includes('-h') ? null : (await import(resolve(exact2, 'scripts/app-shells.mjs')).catch(() => null))?.platformOf(verb, host);
  if (verb === 'ios' && process.platform === 'linux' && rest.includes('--run') && !rest.includes('--help')) {
    console.error('ios --run needs an iOS simulator or connected iPhone; bun exact.mjs ios builds the bundle on Linux/WSL'); return log(2);
  }
  if (platform) {
    const shells = await import(resolve(exact2, 'scripts/app-shells.mjs'));
    const named = shells.namesPlatform(existsSync(manifest) ? JSON.parse(readFileSync(manifest, 'utf8')) : {}, platform);
    const device = rest.includes('--device') || verb === 'share';
    // The Xcode row holds a build to the SDK it compiles with, and only a build that compiles the Swift host to its floor.
    const destination = { only: platform, device, tv: rest.includes('--tvos'), swift: named || await import(resolve(exact2, 'host/apple/devices.mjs')).then(({ compilesSwiftHost }) => compilesSwiftHost(rest), () => true) };
    const report = named ? null : await import(resolve(exact2, 'scripts/exact.mjs')).then(({ sdkReport, platformReport }) => platformReport(platform, sdkReport(process.env, destination), { device })).catch((error) => ({ error: String(error?.message ?? error) }));
    const brought = await shells.bringPlatform(dir, platform, { report, name });
    if (brought.refused) { console.error(brought.refused); reply?.({ code: 'platform_refused', error: brought.refused }); return log(1); }
    for (const line of brought.lines) console.error(line);
  }
  // The automatic build reports on stderr, so a drive's stdout stays its reply (`--json`).
  // The child ends with this process: a signal here is passed on, and a dev server
  // stops when this process is gone however it ended (`EXACT_LAUNCHER_PID`).
  const spawned = (command, args, stdio = 'inherit', cwd = undefined, piped = () => {}, env = {}) => new Promise((done) => {
    const child = spawn(command, args, {
      stdio,
      cwd,
      // EXACT2 as well, for an app's scripts that read that name (LLP 1036.001 D3)
      env: { ...process.env, HERMES_LEAN_SYS_OFFLINE: '1', EXACT_APP_DIR: dir, EXACT: exact2, EXACT2: exact2, EXACT_LAUNCHER_PID: String(process.pid), ...env },
    });
    piped(child);
    const pass = (signal) => { interrupted = true; stopped ??= signal; child.kill(signal); }, signals = ['SIGINT', 'SIGTERM', 'SIGHUP'];
    for (const signal of signals) process.on(signal, pass);
    child.on('error', () => done(1));
    child.on('exit', (code, signal) => {
      for (const s of signals) process.off(s, pass);
      // A child a signal ended (here, or sent to it alone) silences the nudge, whatever a later child returns.
      if (signal || code >= 128) interrupted = true;
      done(code ?? 128 + (constants.signals[signal] ?? 0));
    });
  });
  const script = ([file, ...args], more = [], stdio = 'inherit') => spawned(process.execPath, [resolve(exact2, file), ...args, ...more], stdio);
  const bun = (command) => command === 'bun' ? process.execPath : command;
  // A flag the command's script refuses (scripts/help.mjs), refused here first so that the refusal comes before any work
  const refusedFlag = async () => (await import(resolve(exact2, 'scripts/help.mjs')).catch(() => null))?.guardFlags(verb, rest, { exit: (code) => ({ code }) })?.code;
  // app.json's `before_build`, before a command that builds: with no input and its output on stderr, so a drive's
  // stdout stays its reply. `contract build`, `share` and `verify` under `--json` report a failure on stdout, in their
  // own shapes and with status 1; `contract build --json`, which writes nothing else anywhere, drops the output too. A
  // signal passed to it stops the command even if it then exits 0. exact.mjs run inside it (`EXACT_BEFORE_BUILD`)
  // prepares nothing.
  const jsonFailure = !rest.includes('--json') ? null
    : verb === 'contract' && rest[0] === 'build' ? (message) => [{ id: 'app-before-build', severity: 'error', message, file: manifest, line: 0, col: 0, end_col: 0, related: [] }]
    : verb === 'share' ? (message) => ({ code: 'before_build', error: message })
    : verb === 'verify' ? (message) => ({ error: message, exit: 1 })
    : jsonBuilt ? (message) => ({ code: 'before_build', error: message })
    : null;
  const quiet = verb === 'contract' && jsonFailure != null;
  const prepare = async () => {
    const before = declared.before_build;
    if (before === undefined || process.env.EXACT_BEFORE_BUILD) return 0;
    const refuse = (code, message) => {
      if (!jsonFailure) { console.error(message); return code; }
      (reply ?? ((object) => console.log(JSON.stringify(object))))(jsonFailure(message));
      return 1;
    };
    if (!(Array.isArray(before) && before.length && before.every((part) => typeof part === 'string' && part))) {
      return refuse(2, `app.json before_build is ${JSON.stringify(before)}; it must be a command as argv, such as ["bun", "scripts/fetch.mjs"], so ${verb} did not run`);
    }
    const [command, ...args] = before;
    let failed = null, output = '';
    const ended = [];
    const code = await spawned(bun(command), args, quiet ? ['ignore', 'pipe', 'pipe'] : ['ignore', 2, 2], dir, (child) => {
      child.on('error', (e) => { failed = e; });
      if (quiet) for (const stream of [child.stdout, child.stderr]) {
        stream.setEncoding('utf8');
        stream.on('data', (chunk) => { output += chunk; });
        ended.push(new Promise((done) => stream.on('end', done)));
      }
    }, { EXACT_BEFORE_BUILD: '1' });
    if (stopped) return code || 128 + (constants.signals[stopped] ?? 0);
    if (failed) return refuse(1, `app.json before_build ${JSON.stringify(before)} could not start in ${dir} (${failed.message}): install ${command} or put it on PATH, so ${verb} did not run`);
    // Its last lines, which may arrive after its exit
    await Promise.all(ended);
    const said = quiet ? `: ${output.trim().split('\n').slice(-5).join('\n')}` : jsonFailure ? '; what it said is on stderr' : '; what it said is above';
    if (code) return refuse(code, `app.json before_build ${JSON.stringify(before)} exited ${code} in ${dir}, so ${verb} did not run${said}`);
    return 0;
  };
// exact.mjs runs from any directory, and a file argument is found where you are, else in the app: `app.contract`
// from the app's parent, `app/app.contract` from inside it (app farm wave 1: 13 builds ran one or the other). Each
// file found in the app is said, with the app's directory; one found nowhere is named before the command runs.
const here = process.cwd(), appDir = dir, source = (arg) => /\.(contract|ts|json)$/.test(arg) && !arg.startsWith('-');
const inApp = (arg) => {
  if (!source(arg) || isAbsolute(arg) || existsSync(resolve(arg))) return null;
  const top = `${basename(appDir)}/`, found = [arg, ...(arg.startsWith(top) ? [arg.slice(top.length)] : [])].map((a) => resolve(appDir, a)).find((p) => existsSync(p));
  // `build --json` says nothing on stderr: its diagnostics carry the path taken, or the file it could not read.
  if (rest[0] === 'build' && rest.includes('--json')) return found ?? null;
  if (found) console.error(`exact.mjs: ${arg} is ${found} (the app is ${appDir}; this shell is in ${here})`);
  else console.error(`exact.mjs: no ${arg} in ${here} or in the app, ${appDir}`);
  return found ?? null;
};
if (verb === 'contract') {
  let moved = false;
  for (const [i, arg] of rest.entries()) {
    if (rest[i - 1] === '-o') continue;
    const found = inApp(arg);
    if (found) { rest[i] = found; moved = true; }
  }
  // What a moved source writes goes beside it, as it would have from the app (`types app.contract -o app.contract.d.ts`).
  const o = rest.indexOf('-o'), out = rest[o + 1];
  if (moved && o >= 0 && out && !isAbsolute(out)) rest[o + 1] = resolve(appDir, out.startsWith(`${basename(appDir)}/`) ? out.slice(basename(appDir).length + 1) : out);
}
  // The web build is about a second when nothing changed, so a web drive builds
  // first rather than refusing a stale build. An ios or macos drive builds first
  // when its build is missing or older than its sources, saying so on one line
  // (scripts/agent-build.mjs, LLP 1116.002 D4); a phone's stays explicit.
  const backendFlag = ['test', 'agent', 'verify'].includes(verb) ? rest.indexOf('--backend') : -1;
  if (backendFlag >= 0) {
    const value = rest[backendFlag + 1];
    if (!value || value.startsWith('--')) { console.error('--backend <url>: name the backend origin'); return log(2); }
    (await import('../js/bake/src/typescript.mjs')).selectBackendOrigin(value);
    rest.splice(backendFlag, 2);
  }
  // `--url` with a page served elsewhere drives that page; `--url /path` opens this app's build at a path, so it builds too.
  const drivesWeb = (verb === 'test' || verb === 'agent') && host === 'web'
    && !rest.some((a, i) => (a === '--url' && !rest[i + 1]?.startsWith('/')) || a === '--web-dist');
  // Xcode's builds are a Mac's: on Windows, the verbs that need one say what Windows has instead.
  if (process.platform === 'win32' && (['ios', 'mac'].includes(verb) || ['test', 'agent', 'verify'].includes(verb) && ['ios', 'macos', 'mac'].includes(host))) {
    const message = verb === 'ios' || host === 'ios' ? 'iOS builds run on Linux through WSL; iOS simulator drives need macOS' : `${verb}${verb === 'ios' || verb === 'mac' ? '' : ` ${host}`} needs a Mac (Xcode); on Windows: bun exact.mjs windows --run opens the app, test windows and verify windows check it`;
    console.error(message);
    reply?.({ code: 'platform_refused', error: message });
    return log(2);
  }
  const drivesNative = (verb === 'test' || verb === 'agent') && ['ios', 'macos', 'mac', 'windows'].includes(host) && !rest.includes('--device');
  // `test [host] [file|glob …]`: the test files named (each `.contract`, a glob or `--test <file>`, from the
  // current directory), else app.test.contract and every other `*.test.contract` beside it (an app's tests split
  // by area under the line cap); each runs in turn and any failure fails the command.
  const tests = [];
  for (let i = verb === 'test' ? 0 : rest.length; i < rest.length;) {
    const named = rest[i] === '--test' ? rest.splice(i, 2)[1] : /\.contract$|\*/.test(rest[i]) ? rest.splice(i, 1)[0] : (i++, null);
    if (named == null) continue;
    const found = named.includes('*') ? [...new Bun.Glob(named).scanSync({ absolute: true })].sort() : [inApp(named) ?? resolve(named)];
    if (!found.length) { console.error(`no test file matches ${named}`); return log(2); }
    tests.push(...found);
  }
  if (drivesWeb || drivesNative) {
    // A flag the drive does not take is refused before the build, not after it (scripts/help.mjs).
    // Its refusal ends here, so the command is logged like any other.
    const refused = await refusedFlag();
    if (refused != null) return log(refused);
    // A server entry that does not read DB is refused before the build too, not after it (verify.mjs `withAppServer`, LLP 1116.008 D1).
    const refusal = verb === 'test' ? (await import(resolve(exact2, 'scripts/verify.mjs'))).appServer(dir)?.refusal : null;
    if (refusal) { console.error(`test: ${refusal}`); return log(1); }
    const prepared = await prepare();
    if (prepared) return log(prepared);
    // An `agent` drive's check hashes the build's inputs, and the driver need not again (LLP 1012.001.000 D1: once a drive).
    const checked = verb === 'agent' && drivesNative ? resolve(tmpdir(), `exact-build-check-${process.pid}`) : null;
    if (checked) process.env.EXACT_BUILD_CHECK_OUT = checked;
    // A web build says every compiler warning each time; the drive says the new ones and a count (`warningLines`).
    const built = drivesWeb ? await webBuildQuiet(spawned, [resolve(exact2, verbs['web-build'][0]), ...verbs['web-build'].slice(1)], dir)
      : await script(['scripts/agent-build.mjs', host, '--app', name], [], [0, 2, 2]);
    if (checked) { delete process.env.EXACT_BUILD_CHECK_OUT; try { process.env.EXACT_BUILD_CHECKED = readFileSync(checked, 'utf8'); rmSync(checked); } catch {} }
    if (built) return log(await withCargoFault(built));
  }
  // A verb that builds, runs, compiles or checks the app prepares first (`beforeBuild`); a drive prepared above. An
  // app's own command keeps its name (a game's verify, an app's check) and prepares nothing.
  const prepares = beforeBuild[verb];
  if (declared.before_build !== undefined && !own[verb] && (typeof prepares === 'function' ? prepares(rest) : prepares)) {
    // The verbs whose scripts refuse a flag they do not take (`guardFlags`)
    if (['web', 'web-build', 'serve', 'verify', 'ios', 'mac', 'linux', 'android', 'snapback', 'pattern', 'share'].includes(verb)) {
      const refused = await refusedFlag();
      if (refused != null) return log(refused);
    }
    const prepared = await prepare();
    if (prepared) return log(prepared);
  }
  if (verb === 'contract' && rest[0] === 'build' && existsSync(manifest) && existsSync(resolve(exact2, 'snapback4/web/device-build.mjs'))) {
    const app = { dir, manifest: JSON.parse(readFileSync(manifest, 'utf8')) };
    const { devicePreflight } = await import(resolve(exact2, 'snapback4/web/device-build.mjs'));
    await devicePreflight(app);
  }
  if (own[verb]) { const [command, ...args] = own[verb]; return log(await spawned(bun(command), [...args, ...rest], 'inherit', dir)); }
  // A drive on iOS shows UIKit's bars, as the app and verify do (`--chrome agent` paints the authored header and
  // tablist instead), and an Apple drive's screenshots are a pixel a point: a third of an iPhone's pixels each
  // way, about a third of the tokens an agent reads a picture for (`--scale 3`: the device's). A phone's drive too,
  // which drivesNative leaves out only because its build is explicit.
  const drivesApple = (verb === 'test' || verb === 'agent') && ['ios', 'macos', 'mac'].includes(host);
  if (drivesApple && !rest.includes('--chrome') && host === 'ios') rest.push('--chrome', 'platform');
  if (drivesApple && verb === 'agent' && !rest.includes('--scale')) rest.push('--scale', '1');
  if ((verb === 'test' || verb === 'agent') && host === 'windows' && !rest.includes('--size') && existsSync(manifest)) rest.push('--size', windowsWindow(JSON.parse(readFileSync(manifest, 'utf8'))).join('x'));
  // An Android drive is the APK's (`bun exact.mjs android` installs it), unless it asks for the headless Linux host.
  if ((verb === 'test' || verb === 'agent') && host === 'android' && !rest.includes('--headless') && !rest.includes('--reader')) rest.push('--reader');
  // A finished site is served to this machine unless --lan asks for every interface (LLP 1153 D3.3); the dev loop
  // and serve.mjs run alone keep LAN (LLP 1023 D8).
  if (verb === 'serve' && !rest.includes('--lan')) rest.push('--loopback');
  if (jsonBuilt) return log(await jsonBuild(spawned, [resolve(exact2, verbs[verb][0]), ...verbs[verb].slice(1), ...rest], verb, reply));
  if (verb !== 'test') return log(await withCargoFault(await script(verbs[verb], rest)));
  // An app with a server of its own (the rest pattern's) has it run for the tests, where app.ts calls it, on an empty
  // database kept through them: as verify runs it, by the same code (LLP 1116.007 D2).
  const { withAppServer } = await import(resolve(exact2, 'scripts/verify.mjs'));
  return log(await withAppServer(dir, async () => {
    let failed = 0;
    // None authored: app.test.contract, whose absence the driver names.
    const files = tests.length ? tests : (await import(resolve(exact2, 'scripts/app.mjs'))).authoredTests(dir);
    for (const file of files.length ? files : [resolve(dir, 'app.test.contract')]) {
      if (files.length > 1) console.log(`# ${relative(process.cwd(), file)}`);
      const code = await script(verbs.test, ['--test', file, ...rest]);
      // A signal passed to this file's drive was a request to stop the run, not a failing test.
      if (stopped) return code || 128 + (constants.signals[stopped] ?? 0);
      failed = code || failed;
    }
    return failed;
  }, { verb: 'test', optional: true }));
}

/** The verbs whose build reports its artifact under `--json` (help.mjs `reportBuilt`). */
const JSON_BUILDS = ['ios', 'mac', 'web-build', 'linux', 'windows', 'android'];

/** A build verb under `--json`: the build runs with its stdout on this stderr and writes its result to descriptor 3,
 * and this prints that object, or `{code, error}` when the build failed or named no artifact. The exit code. */
async function jsonBuild(spawned, args, verb, reply) {
  let said = '', ended;
  const code = await spawned(process.execPath, args, ['inherit', 2, 2, 'pipe'], undefined, (child) => {
    child.stdio[3].setEncoding('utf8');
    child.stdio[3].on('data', (chunk) => { said += chunk; });
    ended = new Promise((done) => { child.stdio[3].on('end', done); child.stdio[3].on('error', done); });
  }, { EXACT_JSON_FD: '3' });
  // What it wrote before it exited, unless a process it left running holds the descriptor open
  await Promise.race([ended, new Promise((done) => setTimeout(done, 2000).unref())]);
  let result = null;
  try { result = JSON.parse(said.trim().split('\n').at(-1)); } catch {}
  if (!code && typeof result?.path === 'string') { reply(result); return 0; }
  const error = code ? `${verb} failed (exit ${code}); what it said is on stderr` : `${verb} built nothing it could name; what it said is on stderr`;
  console.error(error);
  reply({ code: 'build_failed', error });
  return code || 1;
}

/** The app's web build, run as a drive's first step: what it says goes to stderr as it comes, except the compiler's
 * warnings, which are said after as `warningLines` gives them. Its exit code. */
async function webBuildQuiet(spawned, args, dir) {
  const warnings = [], streams = [];
  const code = await spawned(process.execPath, args, ['inherit', 'pipe', 'pipe'], undefined, (child) => {
    for (const stream of [child.stdout, child.stderr]) {
      let rest = '';
      const line = (text) => { const found = compilerWarnings(text); if (found.length) warnings.push(...found); else process.stderr.write(`${text}\n`); };
      stream.setEncoding('utf8');
      stream.on('data', (chunk) => { const lines = (rest + chunk).split('\n'); rest = lines.pop(); lines.forEach(line); });
      streams.push(new Promise((done) => stream.on('end', () => { if (rest) line(rest); done(); })));
    }
  }, { EXACT_WARNINGS_WHOLE: '1' }); // each warning, which `warningLines` groups and lists
  await Promise.all(streams);
  for (const said of warningLines(dir, 'web', [...new Set(warnings)])) console.error(said);
  return code;
}

/** What the web build's wasm-bindgen CLI processes in a workspace's graph resolved for wasm32 (`cargo metadata
 * --filter-platform`): the packages `roots` names and their normal dependencies, not dev or build ones. The
 * wasm-bindgen versions there, and each package of the workspace or a path that requires wasm-bindgen, with its
 * requirement. */
export function gpuBindgen(metadata, roots) {
  const byId = new Map(metadata.packages.map((p) => [p.id, p])), nodes = new Map((metadata.resolve?.nodes ?? []).map((n) => [n.id, n]));
  const normal = (dep) => (dep.dep_kinds ?? [{ kind: null }]).some((k) => k.kind === null);
  const seen = new Set(), queue = metadata.workspace_members.filter((id) => roots.includes(byId.get(id)?.name));
  while (queue.length) { const id = queue.pop(); if (!seen.has(id)) { seen.add(id); queue.push(...(nodes.get(id)?.deps ?? []).filter(normal).map((d) => d.pkg)); } }
  const reached = [...seen].map((id) => byId.get(id)).filter(Boolean);
  return {
    held: [...new Set(reached.filter((p) => p.name === 'wasm-bindgen').map((p) => p.version))],
    requiring: reached.filter((p) => !p.source).flatMap((p) => p.dependencies.filter((d) => d.name === 'wasm-bindgen' && !d.kind).map((d) => ({ name: p.name, req: d.req, manifest: p.manifest_path }))),
  };
}

/** A note when the app's GPU modules link a wasm-bindgen other than the CLI the web build runs (`exact setup` installs
 * the version exact2 pins in game/Cargo.toml), which refuses any other; null otherwise. The modules are `<app>-gpu`,
 * in gpu/, and `<app>-gpu-<module>` for each of app.json's `gpu.modules`; with no gpu/ crate the web build
 * (host/web-js/build.mjs) builds none of them. */
function bindgenFinding(dir, workspace, options) {
  const game = resolve(import.meta.dir, '../game/Cargo.toml');
  const pin = existsSync(game) ? String(Bun.TOML.parse(readFileSync(game, 'utf8')).workspace?.dependencies?.['wasm-bindgen'] ?? '').replace(/^=/, '') : '';
  const read = (path) => { try { return Bun.TOML.parse(readFileSync(path, 'utf8')); } catch { return null; } };
  const app = String(read(resolve(dir, 'gpu/Cargo.toml'))?.package?.name ?? '').match(/^(.+)-gpu$/)?.[1];
  if (!pin || !app) return null;
  let modules = []; try { modules = Object.keys(JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8')).gpu?.modules ?? {}); } catch {}
  const wasm = spawnSync('cargo', ['metadata', '--offline', '--locked', '--filter-platform', 'wasm32-unknown-unknown', '--format-version', '1'], { ...options, cwd: workspace });
  if (wasm.status !== 0) return null;
  const { held, requiring } = gpuBindgen(JSON.parse(wasm.stdout), [`${app}-gpu`, ...modules.map((m) => `${app}-gpu-${m}`)]);
  if (!held.length || held.every((version) => version === pin)) return null;
  // Paths as the recipe writes them, with `/` on every host.
  const shown = (path) => relative(workspace, path).split(sep).join('/');
  const blocking = requiring.filter((r) => !Bun.semver.satisfies(pin, r.req)), own = shown(resolve(dir, 'gpu/Cargo.toml'));
  return `The GPU module's wasm build links wasm-bindgen ${held.join(', ')} (${requiring.map((r) => `${r.name} requires ${r.req}`).join(', ')}), and the web build's wasm-bindgen CLI is ${pin}, which refuses any other: the web build will fail. `
    + (blocking.length
      ? `Change ${blocking.map((r) => `${r.name}'s requirement (${shown(r.manifest)})`).join(' and ')} to wasm-bindgen = "=${pin}", then run bun exact.mjs lock again.`
      : `Require wasm-bindgen = "=${pin}" in ${own}'s wasm32 dependencies, then run bun exact.mjs lock again: Cargo moves wasm-bindgen and the crates that name its version with it.`);
}

/** Explicit app lock repair: preserve selected versions and resolve only manifest drift. */
export async function lockApp(dir) {
  const { cargoEnvironment } = await import('./app.mjs');
  const { authoredWorkspace, shellsDir } = await import('./cargo-check.mjs');
  const options = { cwd: dir, env: cargoEnvironment(), encoding: 'utf8', maxBuffer: 1 << 26 };
  // An app that authored no workspace has the generated one once a native verb wrote it (LLP 1153 D1.3), else no Rust.
  const workspace = authoredWorkspace(dir) ?? (existsSync(resolve(shellsDir(dir), 'Cargo.toml')) ? shellsDir(dir) : null);
  if (!workspace) return `${dir}: no Rust here yet; a native verb (bun exact.mjs ios, mac, linux, android, windows) brings it`;
  const lock = resolve(workspace, 'Cargo.lock');
  const before = existsSync(lock) ? readFileSync(lock, 'utf8') : '';
  let result = spawnSync('cargo', ['metadata', '--offline', '--format-version', '1'], { ...options, cwd: workspace });
  if (result.status !== 0 && /no matching package named|--offline was specified|attempting to make an HTTP request|in the offline mode|offline mode \(via `--offline`\)/.test(result.stderr ?? '')) {
    console.error(`${workspace}: fetching missing Cargo sources to resolve the app lock`);
    result = spawnSync('cargo', ['metadata', '--format-version', '1'], { ...options, cwd: workspace });
  }
  if (result.status !== 0) throw new Error(`could not resolve ${lock}: ${result.stderr || result.error?.message}`);
  const { lockChanges } = await import('../game/app/shells.mjs');
  const changes = lockChanges(before, readFileSync(lock, 'utf8'));
  const said = `${lock}: ${changes.length ? `resolved\n${changes.join('\n')}` : 'current; no changes'}`;
  const found = bindgenFinding(dir, workspace, options);
  return found ? `${said}\n${found}` : said;
}
if (import.meta.main && process.argv[2] === '--lock') {
  try { console.log(await lockApp(process.argv[3])); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
