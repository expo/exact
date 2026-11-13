// The file's names: every definition the Contract compiler reports (`contract symbols`) with each use of it, so
// TypeScript answers definition, references, highlights and rename exactly as the compiler resolved them.
// The file exports `the`: a property per definition, keyed by where and what it is, holding a property of the
// definition's own name, typed as the compiler writes its type in TypeScript. A use reads that name through `the`, or
// through the `the` of the `.contract` file that defines it, so two definitions of one name never meet and any name can
// be a property.
import { readFileSync } from 'node:fs';
import { dirname, extname, relative, sep } from 'node:path';

const IDENTIFIER = /^[A-Za-z_$][\w$]*$/;

/** What a definition is, for its hover. */
const about = d => `${d.kind} \`${d.name}\`${d.owner ? ` of ${d.owner}` : ''}${d.component ? ` in component ${d.component}` : ''}${d.writes?.length ? `; writes ${d.writes.map(w => `\`${w}\``).join(', ')}` : ''}`;
const key = d => JSON.stringify(`${d.line}:${d.col}:${d.kind}:${d.name}`);

/** The compiler's span of a symbol in `text`, as `[start, length, quoted]`, when its text is the name and TypeScript
 * can hold it as an identifier, or is the name as a string literal. */
function spelling(ctx, s, text = ctx.content) {
  const at = ctx.span(s, text); if (!at) return null;
  const authored = text.slice(at[0], at[0] + at[1]);
  if (authored === s.name && IDENTIFIER.test(s.name)) return [at[0], at[1], false];
  if (authored === JSON.stringify(s.name)) return [at[0], at[1], true];
  return null;
}

export function names(ctx) {
  const symbols = ctx.contract(['symbols', ctx.fileName, '--stdin', '--editor']) ?? {}, { references = [] } = symbols;
  // Files read on their own may each declare a shape of one name: this file's are its own.
  const shapes = Object.fromEntries((symbols.shapes ?? []).filter(s => s.file === undefined || ctx.here(s)).map(s => [s.name, s.type]));
  const contract = s => s.file !== undefined && extname(s.file) === '.contract';
  const elsewhere = d => !ctx.here(d) && contract(d);
  const text = file => { try { return readFileSync(file, 'utf8'); } catch { return null; } };
  /** A definition's name as its own file writes it. An import that meets a name of this file is `Row__row` to the
   * compiler and `Row` where it is declared, which is the name its file's mapping holds it by. */
  const written = d => {
    const other = elsewhere(d) ? text(d.file) : null, at = other && ctx.span(d, other), authored = at && other.slice(at[0], at[0] + at[1]);
    return authored && IDENTIFIER.test(authored) && d.name.startsWith(`${authored}__`) ? authored : d.name;
  };
  const definitions = (symbols.definitions ?? []).map(d => ({ ...d, name: written(d), compiled: d.name }));
  // Another file is as the compiler read it, from disk; its own mapping places its names.
  const placed = d => { if (ctx.here(d)) return spelling(ctx, d); const other = elsewhere(d) ? text(d.file) : null; return other === null ? null : spelling(ctx, d, other); };
  // A source's name is also the key of the function in app.ts that answers it, which a rename here would not reach.
  const renamable = definitions.map(d => d.kind !== 'source' && placed(d) !== null), used = definitions.map(() => false);
  /** Each place in this file the compiler names a use at, with every definition it says the use is of. */
  const uses = new Map();
  for (const r of references) {
    const d = definitions[r.to], named = r.name === d.compiled, at = ctx.here(r) && named ? spelling(ctx, { ...r, name: d.name }) : null;
    if (at) { const use = uses.get(`${at[0]}:${at[1]}`) ?? uses.set(`${at[0]}:${at[1]}`, { at, of: [] }).get(`${at[0]}:${at[1]}`); use.of.push(r.to); used[r.to] = true; }
    // A use TypeScript is not shown would be left out of a rename. A renamed import's alias (`use Row as TheirRow`)
    // is no use of the name: it stays as it is when the name changes.
    else if (d.compiled === d.name && (ctx.here(r) || !contract(r) || !named)) renamable[r.to] = false;
  }
  // Renaming one of a use's several definitions would leave the use naming the others.
  for (const { of } of uses.values()) if (of.length > 1) for (const i of of) renamable[i] = false;
  const features = of => of.every(i => renamable[i]) ? ctx.FEATURES : ctx.FEATURES & ~ctx.FEATURE.RENAME;
  const name = (at, of) => ctx.name(ctx.content.slice(at[0], at[0] + at[1]), at, features(of));

  const from = new Map(), entry = i => `${elsewhere(definitions[i]) ? from.get(definitions[i].file) : 'the'}[${key(definitions[i])}]`;
  definitions.forEach((d, i) => {
    if (!used[i] || !elsewhere(d) || from.has(d.file)) return;
    // The compiler names the other file with links resolved; the import is from this file's own directory, as TypeScript knows it.
    const path = relative(ctx.real(dirname(ctx.fileName)), ctx.real(d.file)).split(sep).join('/');
    from.set(d.file, `$${from.size}`);
    ctx.write(`import { the as ${from.get(d.file)} } from ${JSON.stringify(path.startsWith('.') ? path : `./${path}`)};\n`);
  });
  // What the compiler gives no type reads as what it is. A component reads as its members, each with the compiler's
  // type; any other kind as an empty interface named for it, unless the file has a shape of that name.
  const KINDS = { route: 'Route', source: 'Source', testId: 'TestId', id: 'Id', style: 'Style', task: 'Task', font: 'Font', keyframes: 'Keyframes', atom: 'Atom', decoration: 'Decoration' };
  const taken = new Set([...Object.keys(shapes), ...definitions.filter(d => d.kind === 'shape').map(d => d.name)]);
  for (const name of new Set(Object.values(KINDS))) if (!taken.has(name)) ctx.write(`interface ${name} {}\n`);
  const members = (component, kind) => definitions.filter(d => d.component === component && d.kind === kind && d.type && IDENTIFIER.test(d.name)).map(d => `${d.name}: ${d.type}`);
  const untyped = d => {
    if (d.kind === 'component') { const parts = ['prop', 'state', 'derive', 'resource', 'mutation', 'action'].map(k => [k, members(d.name, k)]).filter(([, m]) => m.length); return parts.length ? `{ ${parts.map(([k, m]) => `${k === 'derive' ? 'derives' : k === 'state' ? 'state' : `${k}s`}: { ${m.join('; ')} }`).join('; ')} }` : 'unknown'; }
    const name = KINDS[d.kind]; return name && !taken.has(name) ? name : 'unknown';
  };
  // The shapes the definitions' types name, so a hover reads `Greeting` and not its fields.
  for (const [shape, body] of Object.entries(shapes)) if (IDENTIFIER.test(shape)) ctx.write(`type ${shape} = ${body};\n`);
  ctx.write('export declare const the: {\n');
  definitions.forEach((d, i) => {
    if (!ctx.here(d) && !(used[i] && !elsewhere(d))) return;
    ctx.write(`${key(d)}: {\n/** ${about(d).replace(/\*\//g, '* /')} */\n`);
    const at = ctx.here(d) ? spelling(ctx, d) : null;
    if (at) name(at, [i]); else ctx.write(JSON.stringify(d.name));
    ctx.write(`: ${d.type ?? untyped(d)} };\n`);
  });
  ctx.write('};\n');
  for (const { at, of } of uses.values()) {
    // A use of several definitions reads its name from their union, so TypeScript's definition is all of them.
    ctx.write(of.length === 1 ? entry(of[0]) : `(null! as ${of.map(i => `typeof ${entry(i)}`).join(' | ')})`);
    if (at[2]) { ctx.write('['); name(at, of); ctx.write(']'); } else { ctx.write('.'); name(at, of); }
    ctx.write(';\n');
  }
}
