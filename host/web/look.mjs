// The web's neutral presentation, composed after the shared browser resets.
// @ref LLP 1115.000 D4 part B
import { readFileSync } from 'node:fs';
import { parseColour, schemeColours } from '../../scripts/scheme-colours.mjs';

/** The shell inputs recorded by a web build's freshness receipt. */
export const SHELL_SOURCES = ['host/web/index.html', 'host/web/neutral.css', 'host/web/look.mjs', 'host/web/scheme.js', 'kernel/tables/schema.json', 'scripts/scheme-colours.mjs'];

/** Append the neutral sheet before the build minifies the shell's styles. */
export function withLook(shell) {
  const at = shell.indexOf('</style>');
  if (at < 0) throw new Error('the shell has no <style> to carry the neutral look');
  return shell.slice(0, at) + readFileSync(new URL('./neutral.css', import.meta.url), 'utf8') + appleColors() + shell.slice(at);
}

/** Exact's colour roles in Apple's palette, in every browser (LLP 1115.000 D4): the schema's pairs, measured from
 * UIKit, and in Safari the system's own where WebKit names one (`-apple-system-*`). Each is held as
 * `--exact-apple-<role>`, which an app's theme (LLP 1095 D4) leaves alone, for the glue to report to the kernel;
 * `--exact-apple-roles` names them. */
export function appleColors() {
  const { colors } = JSON.parse(readFileSync(new URL('../../kernel/tables/schema.json', import.meta.url), 'utf8'));
  // Exact's roles (CSS's system colours are the browser's), less the tint's, which the accent draws
  const roles = colors.filter(([name, , ios]) => /^[a-z]/.test(name) && !ios.startsWith('@tint'));
  const pairs = roles.map(([name, , , , light, dark]) => `--exact-apple-${name}: light-dark(${light}, ${dark}); --exact-${name}: var(--exact-apple-${name});`).join(' ');
  // The backgrounds are UIKit's: WebKit on a Mac resolves all six to one colour, so a card would not stand out
  const own = roles.filter(([name, alias]) => alias && !name.endsWith('background')).map(([name, alias]) => `  @supports (color: ${alias}) { :root { --exact-apple-${name}: ${alias}; } }\n`);
  return `  :root { ${pairs} --exact-apple-roles: "${roles.map(([name]) => name).join(' ')}"; }\n${own.join('')}`;
}

/** The manifest's fixed `userInterfaceStyle` (`light` or `dark`), or null for `automatic` or none. */
export const interfaceStyle = (manifest) => (['light', 'dark'].includes(manifest?.userInterfaceStyle) ? manifest.userInterfaceStyle : null);

/** The page's `color-scheme` before its first paint: the shell's `light dark`, or a fixed `style` (`interfaceStyle`), which
 * then also leads the head as `<meta name="color-scheme">` so the browser's canvas is in it before any style is read. The
 * scheme is the page's, as the device's would be: `setScheme("system")` returns to it, and the runner's
 * `prefers-color-scheme` reads it from the meta (navigation.js `preferences`). */
export function withInterfaceStyle(shell, style) {
  if (!style) return shell;
  const rule = 'html { color-scheme: light dark; }';
  if (!shell.includes(rule)) throw new Error(`the shell has no \`${rule}\` to fix as ${style}`);
  return shell.replace(rule, `html { color-scheme: ${style}; }`).replace(/(<meta name="viewport"[^>]*>\n)/, `$1${colorSchemeMeta(style)}`);
}

/** The head's `<meta name="color-scheme">` for a fixed `style`; nothing for none. */
export const colorSchemeMeta = (style) => (style ? `<meta name="color-scheme" content="${style}">\n` : '');

/** Restore the app's stored scheme before the runtime or the first paint. */
export const schemeHead = (key, { development = false, wasm = false } = {}) => `${key ? `<meta name="exact-scheme" content="${key}"${development ? ' data-development' : ''}${wasm ? ' data-wasm' : ''}>\n` : ''}<script data-exact-scheme>${readFileSync(new URL('./scheme.js', import.meta.url), 'utf8').trimEnd()}</script>\n`;

/** The manifest's launch colour in each scheme it names one for (`schemeColours`, LLP 1163 D2), until the app paints
 * its root (LLP 1034 D2), and the shell's page (`--exact-background`, neutral.css, which is one colour in every engine
 * where each engine's dark `Canvas` is its own) in a scheme it names none for. A Display P3 colour is written as CSS
 * has it. */
export function launchBackground(manifest) {
  const { light, dark } = schemeColours(manifest, 'background_color');
  if (!light && !dark) return '';
  for (const [scheme, text] of Object.entries({ light, dark })) if (text) parseColour(text, `the ${scheme} launch colour`);
  const page = 'var(--exact-background)';
  const color = light === dark ? light : `light-dark(${light ?? page},${dark ?? page})`;
  return `html:has(#exact-root:empty){background-color:${color}}`;
}
