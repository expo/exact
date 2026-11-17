// The UTI each `file_handlers` MIME type names on Apple platforms (LLP 1033
// D1). `inode/directory` is the one deviation from IANA's registry —
// freedesktop's spelling for a folder, because the web has no MIME type for
// one and an app that opens a directory (the LLP reader) must be able to say
// so. An unmapped type is refused rather than guessed (LLP 0382: fail
// closed, loudly), when any build reads the manifest: the web's too, so a
// manifest the web accepts is one every host builds (files diary F12). The
// common document and image types are here (ledger diary F9), and IANA's
// generic binary is Apple's generic data (files diary F12: `.bin`, `.dat`).
import documentUTIs from './document-utis.json' with { type: 'json' };
export const DOCUMENT_UTIS = documentUTIs;

/** An app's own document format (studio diary R13): a MIME type in IANA's
 * vendor or unregistered trees (`application/vnd.studio.board+json`,
 * `application/x-studio-board`), which no system type names. The Apple
 * bake exports it as the app's: `<app id>.<subtype>`, conforming to what
 * its structured-syntax suffix says (`+json` is JSON) and to data; the app
 * is its editor and owner. `null` for any other type. */
export function ownDocumentType(appId, mime) {
  const m = /^[a-z]+\/(?:x-|vnd\.|prs\.)([a-z0-9][a-z0-9.+-]*)$/i.exec(mime);
  if (!m || Object.hasOwn(DOCUMENT_UTIS, mime)) return null;
  const [subtype, suffix] = m[1].toLowerCase().split('+');
  const base = { json: 'public.json', xml: 'public.xml', zip: 'public.zip-archive' }[suffix];
  return {
    identifier: `${appId}.${subtype.replace(/[^a-z0-9.-]/g, '-')}`,
    conformsTo: [...(base ? [base] : mime.startsWith('text/') ? ['public.plain-text'] : ['public.data']), 'public.content'],
  };
}
export function documentTypeProblems(manifest) {
  const control = (value) => [...value].some((c) => c.codePointAt(0) <= 0x1f || (c.codePointAt(0) >= 0x7f && c.codePointAt(0) <= 0x9f));
  const extensionProblem = (value) => { const suffix = value.startsWith('.') ? value : `.${value}`; return suffix.length > 16 || suffix.endsWith('.') || !/^\.[A-Za-z0-9+.]+$/.test(suffix) ? 'is not a file suffix: use at most 16 ASCII alphanumeric, "+" or "." code points, with an optional leading dot and no trailing dot' : null; };
  return (manifest.file_handlers ?? []).flatMap((handler, i) => {
    const problems = handler.name !== undefined && (!handler.name || control(handler.name)) ? [`file_handlers[${i}].name: ${JSON.stringify(handler.name)} is not a label: use a non-empty string with no control character`] : [];
    for (const [mime, extensions] of Object.entries(handler.accept)) {
      extensions.forEach((extension, j) => { const problem = extensionProblem(extension); if (problem) problems.push(`file_handlers[${i}].accept.${mime}[${j}]: ${JSON.stringify(extension)} ${problem}`); });
      const own = !Object.hasOwn(DOCUMENT_UTIS, mime) && ownDocumentType('app', mime);
      if (!Object.hasOwn(DOCUMENT_UTIS, mime) && !own) problems.push(`file_handlers[${i}].accept: ${mime} names no type the Apple hosts map (they map ${Object.keys(DOCUMENT_UTIS).join(', ')}), nor an app's own (application/vnd.<app>.<format> or application/x-<format>, +json for JSON)`);
      else if (own && !extensions.some((extension) => !extensionProblem(extension))) problems.push(`file_handlers[${i}].accept: ${mime}, the app's own type, names no extension (".board")`);
    }
    return problems; });
}
export function canonicalizeDocumentExtensions(manifest) { for (const handler of manifest.file_handlers ?? []) for (const [mime, extensions] of Object.entries(handler.accept)) handler.accept[mime] = extensions.map((extension) => `${extension.startsWith('.') ? '' : '.'}${extension}`.toLowerCase()); }
