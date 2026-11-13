// The TypeScript 7 extension sends a file to TypeScript's server only when an extension has registered the file's
// extension with it. This registers `.contract`, so a project's content mapper (editors/content-mapper) is asked
// about a contract opened on its own. It does so only once the workspace turns content mappers on; until then, or
// without that extension or its mapper API, it does nothing and wakes no other extension.
const TYPESCRIPT = 'TypeScriptTeam.native-preview', ID = 'exact.exact-contract', SETTING = 'js/ts.contentMappers.enabled';

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

/** Registers now, and again when the setting is turned on later. */
function activate(context, vscode = require('vscode')) {
  const state = {};
  context.subscriptions.push(vscode.workspace.onDidChangeConfiguration(e => { if (e.affectsConfiguration(SETTING)) register(vscode, context, state); }));
  return register(vscode, context, state);
}

module.exports = { register, activate };
