// A `.contract` file as TypeScript reads it. The Contract compiler says what the file defines and where each name is
// used (`contract symbols`) and what is wrong with it (`contract build --errors-only`); this module writes those facts
// as TypeScript text and maps each name back to its place, so TypeScript's own language service answers definition,
// references, rename and hover in the authored file. Nothing here parses Contract.
import { spawnSync } from 'node:child_process';
import { existsSync, realpathSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const VERBATIM = 0, IGNORE = 0, NO_COMPILER = 9199;
// What a mapped name answers: everything a reader asks of a name, and no edits but a rename.
/** What a mapping answers, as TypeScript numbers them. */
const FEATURE = { HOVER: 1 << 0, COMPLETION: 1 << 2, DEFINITION: 1 << 3, TYPE_DEFINITION: 1 << 4, IMPLEMENTATION: 1 << 5, REFERENCES: 1 << 6, HIGHLIGHTS: 1 << 7, RENAME: 1 << 8 };
const FEATURES = FEATURE.HOVER | FEATURE.DEFINITION | FEATURE.TYPE_DEFINITION | FEATURE.REFERENCES | FEATURE.HIGHLIGHTS | FEATURE.RENAME;

/** The `contract` binary: the mapper's `contract` option, `EXACT_CONTRACT`, else the checkout's own build. */
function compiler(options) {
  const named = options?.contract ?? process.env.EXACT_CONTRACT; if (named) return named;
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const exe = process.platform === 'win32' ? '.exe' : '';
  return ['target/release/contract', 'target/debug/contract'].map(p => resolve(root, p + exe)).find(existsSync) ?? 'contract';
}
/** The compiler's JSON answer, or none; `failed` is told when it could not be run at all. */
const ask = (bin, args, content, failed) => {
  const r = spawnSync(bin, args, { input: content, encoding: 'utf8', maxBuffer: 64 << 20, timeout: 20000 });
  if (r.error) failed(r.error.code === 'ETIMEDOUT', r.error.code === 'ENOENT' ? `the Contract compiler was not found at \`${bin}\`; set the mapper's \`contract\` option or EXACT_CONTRACT` : `the Contract compiler did not answer: ${r.error.message}`);
  try { return JSON.parse(r.stdout); } catch { return null; }
};
/** Offsets of each line's start, so a 1-based line and column is an offset. */
/** A path as the compiler names it: it resolves links in every path but the one it is given. */
const real = path => { try { return realpathSync.native(path); } catch { return resolve(path); } };
const lines = text => { const at = [0]; for (let i = 0; i < text.length; i++) if (text[i] === '\n') at.push(i + 1); return at; };

/** What each part of the mapping is given: the file, the compiler's answers, and where to write.
 * `name(text, [start, length])` writes `text` mapped to that place of the authored file; `write` writes unmapped text. */
function context(fileName, content, options) {
  const bin = compiler(options), starts = new Map([[content, lines(content)]]), out = { text: '', mappings: [], diagnostics: [] }, asked = new Map();
  // Said once, on the file's first character: with no compiler there is nothing else to say about the file.
  // A compiler that timed out once is not asked again for this file: the parts would each wait in turn.
  let dead = false;
  const failed = (timedOut, message) => { dead ||= timedOut; if (!out.diagnostics.some(d => d.code === NO_COMPILER)) out.diagnostics.push({ start: 0, length: 1, code: NO_COMPILER, messageText: message }); };
  // The compiler's columns count UTF-8 bytes; offsets here count UTF-16 units, as TypeScript's do.
  const offset = (line, col, text = content) => {
    const at = starts.get(text) ?? starts.set(text, lines(text)).get(text), start = at[line - 1] ?? text.length;
    return start + Buffer.from(text.slice(start, at[line] ?? text.length)).subarray(0, col - 1).toString().length;
  };
  return { fileName, content, options, out, offset, real, FEATURE, FEATURES,
    /** One run of the compiler for each question: the parts ask the same ones. */
    contract: (args, input = content) => { const key = JSON.stringify([args, input === content ? 0 : input]); if (!asked.has(key)) asked.set(key, dead ? null : ask(bin, args, input, failed)); return asked.get(key); },
    /** Whether a symbol or an error the compiler names is in this file. */
    here: s => s.file === undefined || s.file === fileName || real(resolve(dirname(fileName), s.file)) === real(fileName),
    /** The span the compiler gives a symbol, as `[start, length]` in `text` (this file's unless given); none if it gives no place. */
    span: (s, text = content) => { if (!(s.line > 0 && s.col > 0)) return null; const start = offset(s.line, s.col, text); return [start, offset(s.line, s.end_col, text) - start]; },
    write: text => { out.text += text; },
    name: (text, at, features = FEATURES) => { out.mappings.push([out.text.length, at[1], at[0], at[1], VERBATIM, features]); out.text += text; } };
}

// The parts, each in its own file: what the file's names are (names.js), its sources in app.ts (sources.js), the
// compiler's errors (errors.js), and the view's vocabulary (vocab.js).
import { names } from './names.js';
import { sources } from './sources.js';
import { errors } from './errors.js';
import { vocab } from './vocab.js';
const PARTS = [names, sources, errors, vocab];

export function transform(fileName, content, options) {
  const ctx = context(fileName, content, options);
  ctx.write('// Generated from the Contract compiler\'s analysis of the authored file.\n');
  // A part that fails leaves the others' answers standing; its text so far is kept, as each write is whole.
  for (const part of PARTS) try { part(ctx); } catch (error) { console.error(`contract content mapper: ${part.name}: ${error?.stack ?? error}`); }
  const { text, mappings, diagnostics } = ctx.out;
  // TypeScript's own errors on this text mean nothing to the author: the Contract compiler's are the file's errors.
  return { text, extension: '.tsx', mappings, diagnostics, diagnosticDirectives: { unusedExpectDirectiveDiagnostics: [], directives: [[0, content.length, 0, text.length, IGNORE]] } };
}

export function createMapper() {
  const projects = new Map();
  return {
    // Every offset here counts UTF-16 units, so that is the one encoding answered.
    initialize: () => ({ positionEncoding: 'utf-16', diagnosticSource: 'contract' }),
    openProject: ({ projectHandle, options }) => { projects.set(projectHandle, options ?? {}); return {}; },
    closeProject: ({ projectHandle }) => { projects.delete(projectHandle); },
    transform: ({ fileName, content, projectHandle }) => transform(fileName, content, projects.get(projectHandle)),
  };
}
