/** app.json's text as JSON. A byte-order mark, which some Windows editors write and JSON does not allow, is named
 * with its fix rather than left to the parser's "Unrecognized token" (expo/exact-build#190). */
export function parseManifest(text) {
  if (text.charCodeAt(0) === 0xfeff) throw new Error('starts with a byte-order mark (U+FEFF), which JSON does not allow; save it as UTF-8 without a BOM');
  return JSON.parse(text);
}
