// The TypeScript 7 extension sends a file to TypeScript's server only when an extension has registered the file's
// extension with it. This registers `.contract`, so a project's content mapper (editors/content-mapper) is asked
// about a contract opened on its own. Without that extension, or its mapper API, this does nothing.
const TYPESCRIPT = 'TypeScriptTeam.native-preview', ID = 'exact.exact-contract';

/** Registers `.contract` with the TypeScript 7 extension among `extensions` (`vscode.extensions`), if it is there. */
async function register(extensions, context) {
  const api = await extensions.getExtension(TYPESCRIPT)?.activate();
  const registration = api?.registerContentMappers?.(ID, [{ extensions: ['.contract'] }]);
  if (registration) context.subscriptions.push(registration);
  return Boolean(registration);
}

module.exports = { register, activate: context => register(require('vscode').extensions, context) };
