// The TypeScript 7 extension sends a file to TypeScript's server only when an extension has registered the file's
// extension with it. This registers `.contract`, so a project's content mapper (editors/content-mapper) is asked
// about a contract opened on its own. It does so only once the workspace turns content mappers on; until then, or
// without that extension or its mapper API, it does nothing and wakes no other extension.
//
// Once the workspace names the Contract compiler in `exactContract.compiler`, it also formats a contract with it
// (`contract fmt --stdin`) and shows the compiler's warnings on each open contract as problems. TypeScript's content
// mappers report errors only, so a warning has no other way into the editor. Until a compiler is named no formatter
// is registered, no document is listened for, and nothing is run.
const { execFile } = require('node:child_process');
const { existsSync, realpathSync } = require('node:fs');
const { basename, dirname, resolve } = require('node:path');

const TYPESCRIPT = 'TypeScriptTeam.native-preview', ID = 'exact.exact-contract', SETTING = 'js/ts.contentMappers.enabled';
const LANGUAGE = 'exact-contract', COMPILER = 'exactContract.compiler';

/** Registers `.contract` with the TypeScript 7 extension, if the setting is on and that extension is installed.
 * `vscode` is the API (`extensions`, `workspace`); a registration is made at most once. */
async function register(vscode, context, state = {}) {
  if (state.registered || !vscode.workspace.getConfiguration().get(SETTING)) return false;
  const api = await vscode.extensions.getExtension(TYPESCRIPT)?.activate();
  const registration = api?.registerContentMappers?.(ID, [{ extensions: ['.contract'] }]);
  if (!registration || state.registered) { registration?.dispose(); return false; }
  state.registered = true;
  context.subscriptions.push(registration);
  return true;
}

/** `text` as `contract fmt --stdin` writes it, the compiler being `bin`; none when the compiler refuses the text or
 * cannot be run, and `missing` is told when it is not there. `file` names the text and is not read. */
const formatted = (bin, file, text, missing) => new Promise(done => {
  const child = execFile(bin, ['fmt', '--stdin', file], { maxBuffer: 64 << 20 }, (error, stdout) => {
    if (error?.code === 'ENOENT') missing();
    done(error ? null : stdout);
  });
  child.stdin.on('error', () => {}); // a compiler that did not start has no stdin to write; `execFile` reports it
  child.stdin.end(text);
});

/** Keeps a formatter for contracts registered while the workspace names a compiler, and none otherwise. */
function format(vscode, state) {
  state.formatter?.dispose();
  state.formatter = undefined;
  const bin = vscode.workspace.getConfiguration().get(COMPILER);
  if (!bin) return;
  let said = false;
  const missing = () => {
    if (!said) vscode.window.showWarningMessage(`Exact Contract: \`${COMPILER}\` names no Contract compiler: ${bin}`);
    said = true;
  };
  state.formatter = vscode.languages.registerDocumentFormattingEditProvider(LANGUAGE, {
    // A refused text gets no edit: its error is the compiler's, shown where its others are.
    async provideDocumentFormattingEdits(document) {
      const text = document.getText(), after = await formatted(bin, document.fileName, text, missing);
      if (after === null || after === text) return [];
      return [vscode.TextEdit.replace(new vscode.Range(document.positionAt(0), document.positionAt(text.length)), after)];
    },
  });
}

/** The compiler's JSON answer to `args`, `text` being its stdin; none when it gives none. A build that refuses its
 * text still answers, so the exit code is not read. */
const asked = (bin, args, text) => new Promise(done => {
  const child = execFile(bin, args, { maxBuffer: 64 << 20 }, (_, stdout) => { try { done(JSON.parse(stdout)); } catch { done(null); } });
  child.stdin.on('error', () => {});
  child.stdin.end(text);
});
/** A path as the compiler names it, with links resolved. */
const real = path => { try { return realpathSync.native(path); } catch { return resolve(path); } };

/** The app a file is built as part of: the nearest `app.contract` beside or above it whose sources the compiler
 * says include it. None for an app, and for a file no app uses, which is built alone. */
async function appOf(bin, file) {
  if (basename(file) === 'app.contract') return null;
  for (let dir = dirname(file), above; ; dir = above) {
    const app = resolve(dir, 'app.contract');
    if (existsSync(app) && ((await asked(bin, ['sources', app], ''))?.sources ?? []).some(s => real(s.path) === real(file))) return app;
    if ((above = dirname(dir)) === dir) return null;
  }
}

/** The compiler's warnings on a file whose text the editor holds, as VS Code's problems. Its errors are left to the
 * content mapper, which shows them through TypeScript. */
async function warnings(vscode, bin, file, text) {
  const app = await appOf(bin, file), lines = text.split('\n');
  const found = await asked(bin, ['build', app ?? file, '--json', ...(app ? ['--stdin-as', file] : ['--stdin'])], text);
  /** The compiler's column, which counts UTF-8 bytes, as the UTF-16 units VS Code counts. */
  const units = (line, col) => Buffer.from(lines[line - 1] ?? '').subarray(0, col - 1).toString().length;
  return (found ?? []).filter(e => e.severity === 'warning' && e.line > 0 && real(e.file) === real(file)).map(e => {
    const range = new vscode.Range(e.line - 1, units(e.line, e.col), e.line - 1, units(e.line, e.end_col ?? e.col + 1));
    return Object.assign(new vscode.Diagnostic(range, e.message, vscode.DiagnosticSeverity.Warning), { code: e.id, source: 'contract' });
  });
}

/** Shows each open contract's warnings while the workspace names a compiler, and nothing otherwise. */
function problems(vscode, state) {
  state.problems?.dispose();
  state.problems = undefined;
  const bin = vscode.workspace.getConfiguration().get(COMPILER);
  if (!bin) return;
  const shown = vscode.languages.createDiagnosticCollection(LANGUAGE), latest = new Map(), compiling = new Map();
  // One compile of a document at a time, and of its newest text only: a text typed over while it waited is skipped,
  // and one typed over while it compiled is not shown. Its places are read in the text that was compiled.
  const check = document => {
    // A file or an unsaved editor's: not a diff's or another provider's copy of a contract.
    if (document.languageId !== LANGUAGE || !['file', 'untitled'].includes(document.uri.scheme)) return;
    const key = String(document.uri), turn = {};
    latest.set(key, turn);
    const done = (compiling.get(key) ?? Promise.resolve()).catch(() => {}).then(async () => {
      if (latest.get(key) !== turn) return;
      const found = await warnings(vscode, bin, document.fileName, document.getText());
      if (latest.get(key) === turn) shown.set(document.uri, found);
    });
    compiling.set(key, done);
    return done;
  };
  const listening = [
    vscode.workspace.onDidOpenTextDocument(check),
    vscode.workspace.onDidChangeTextDocument(change => check(change.document)),
    vscode.workspace.onDidCloseTextDocument(document => { latest.delete(String(document.uri)); shown.delete(document.uri); }),
  ];
  state.problems = { dispose() { for (const l of [...listening, shown]) l.dispose(); } };
  vscode.workspace.textDocuments.forEach(check);
}

/** Registers now, and again when either setting changes. */
function activate(context, vscode = require('vscode')) {
  const state = {};
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(e => {
      if (e.affectsConfiguration(SETTING)) register(vscode, context, state);
      if (e.affectsConfiguration(COMPILER)) { format(vscode, state); problems(vscode, state); }
    }),
    { dispose: () => { state.formatter?.dispose(); state.problems?.dispose(); } },
  );
  format(vscode, state);
  problems(vscode, state);
  return register(vscode, context, state);
}

module.exports = { register, activate };
