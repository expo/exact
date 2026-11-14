// The view's vocabulary: each element the Contract compiler reports (`contract symbols`, `elements`) is written as a
// JSX element against declarations generated from `contract vocab --json --all`, so TypeScript's own hover shows a tag's
// or attribute's documentation and type, and its completion offers an enum attribute's values.
// TypeScript completes only where the cursor falls in a verbatim span, so a string value is written as authored.
const ATOM = 1;
const JSX_NAME = /^[A-Za-z_$][\w$-]*$/;

const cached = new Map();
/** The vocabulary's declarations, once per compiler binary. */
function declarations(ctx) {
  const key = ctx.options?.contract ?? process.env.EXACT_CONTRACT ?? '';
  if (cached.has(key)) return cached.get(key);
  const declared = declare(ctx.contract(['vocab', '--json', '--all'], ''));
  if (declared) cached.set(key, declared); // a compiler that did not answer is asked again
  return declared;
}

const doc = lines => `/**\n${lines.filter(Boolean).join('\n\n').replace(/\*\//g, '* /').split('\n').map(l => ` * ${l}`).join('\n')}\n */`;
const code = s => `\`${s}\``;
const row = r => `${code(r.row)}: ${r.codec}${r.values.length ? ` (${r.values.map(code).join(', ')})` : ''}${r.default !== null ? `, default ${code(r.default)}` : ''}`;
const only = a => a.only && `Only on ${a.only}.`;

function about(a) {
  const head = `${a.kind} attribute ${code(a.name)}`;
  switch (a.kind) {
    case 'prop': case 'inverted-prop': return [`${head}: sets ${code(a.prop)}, ${a.type}${a.kind === 'inverted-prop' ? ', inverted' : ''}.`, a.accepts && `Accepts ${a.accepts}.`];
    case 'handler': return [`${head}: the ${code(a.event)} event.`, a.calls && `${a.calls}.`];
    case 'record': return [`${head}: ${a.type}; sets ${a.props.map(code).join(', ')}.`];
    default: return [`${head}${a.rows?.length ? `, ${a.rows.length === 1 ? 'row' : 'rows'}:` : '.'}`, ...(a.rows ?? []).map(row)];
  }
}

const SCALAR = { str: 'string', string: 'string', bool: 'boolean', int: 'number', float: 'number', number: 'number' };
const NUMERIC = new Set(['f32', 'u8', 'u16', 'u32', 'i32']);
/** A type the vocabulary names, in TypeScript; a name that is not a scalar is declared as an empty interface. */
function named(type, names) {
  const option = /^option<(.+)>$/.exec(type);
  if (option) return `${named(option[1], names)} | null`;
  if (SCALAR[type]) return SCALAR[type];
  const [, name, list = ''] = /^([A-Z]\w*)(\[\])?/.exec(type) ?? [];
  if (!name) return 'string';
  names.add(name);
  return name + list;
}
const rowType = r => r.codec === 'enum' && r.values.length ? r.values.map(v => JSON.stringify(v)).join(' | ') : NUMERIC.has(r.codec) ? 'number' : r.codec === 'dimension' ? 'number | string' : 'string';
/** What an attribute's value is: a handler takes an action of its `params`, or of those and its `record`. */
function valueType(a, names) {
  if (a.kind === 'handler') {
    // A handler whose parameters depend on its element has none listed.
    if (!a.params) return '(...args: never[]) => void';
    const form = params => `((${params.map(p => `${p.name}: ${named(p.type, names)}`).join(', ')}) => void)`;
    return [a.params, ...(a.record ? [[...a.params, { name: 'event', type: a.record }]] : [])].map(form).join(' | ');
  }
  if (a.type) return named(a.type, names);
  const rows = [...new Set((a.rows ?? []).map(rowType))];
  return rows.length === 1 ? rows[0] : rows.length ? 'number | string' : 'string';
}

function declare(vocab) {
  if (!vocab?.tags) return null;
  const out = ['export {};', 'interface $Attributes {'];
  const named = new Set(), types = vocab.attributes.map(a => valueType(a, named));
  out.splice(1, 0, ...[...named].map(n => `interface ${n} {}`));
  vocab.attributes.forEach((a, i) => out.push(doc([...about(a), a.note, a.iframe, only(a)]), `${JSON.stringify(a.name)}?: ${types[i]};`));
  out.push('}', 'type $Element = (attributes: $Attributes) => null;', 'interface $Tags {');
  for (const t of vocab.tags) {
    const fixed = [...t.fixedProps.map(p => `${p.prop}=${p.value}`), ...t.fixedStyles.map(s => `${s.row}: ${s.value}`)];
    out.push(doc([`tag ${code(t.name)}: a ${t.node} (${t.family})${t.positional ? `; its argument is ${code(t.positional)}` : ''}.`, fixed.length && `Sets ${fixed.map(code).join(', ')}.`, t.note]), `${JSON.stringify(t.name)}: $Element;`);
  }
  out.push('}', 'declare const tags: $Tags;', 'declare const $value: never;', '');
  return out.join('\n');
}

export function vocab(ctx) {
  const { HOVER, COMPLETION } = ctx.FEATURE;
  const elements = (ctx.contract(['symbols', ctx.fileName, '--stdin', '--editor'])?.elements ?? []).filter(ctx.here);
  const declared = elements.length && declarations(ctx);
  if (!declared) return;
  ctx.write(declared);
  /** The offset of a 1-based line and the compiler's column there, which counts UTF-8 bytes. */
  const offset = (line, col) => { const start = ctx.offset(line, 1), end = ctx.content.indexOf('\n', start); return start + Buffer.from(ctx.content.slice(start, end < 0 ? undefined : end)).subarray(0, col - 1).toString().length; };
  /** Where the compiler's place is, if the authored text there is `text`. */
  const at = (p, text) => { const start = offset(p.line, p.col); return ctx.content.slice(start, start + text.length) === text && offset(p.line, p.end_col) - start === text.length ? [start, text.length] : null; };
  for (const e of elements) {
    ctx.write('<tags.');
    const tag = at(e, e.tag);
    if (tag) ctx.name(e.tag, tag, HOVER | COMPLETION);
    else { const start = offset(e.line, e.col); ctx.out.mappings.push([ctx.out.text.length, e.tag.length, start, offset(e.line, e.end_col) - start, ATOM, HOVER]); ctx.write(e.tag); }
    const spread = [];
    for (const a of e.attrs) {
      const name = at(a, a.name);
      if (!name) continue;
      const value = a.string && at(a.string, `"${a.string.value}"`);
      if (!JSX_NAME.test(a.name)) { spread.push([a, name, value]); continue; }
      ctx.write(' '); ctx.name(a.name, name, HOVER | COMPLETION); ctx.write('=');
      if (value) ctx.name(`"${a.string.value}"`, value, COMPLETION); else ctx.write('{$value}');
    }
    // A name JSX cannot spell is a key of a spread object, which TypeScript types as the element's attributes.
    if (spread.length) {
      ctx.write(' {...{');
      for (const [a, name, value] of spread) {
        ctx.write('"'); ctx.name(a.name, name, HOVER); ctx.write('": ');
        if (value) ctx.name(`"${a.string.value}"`, value, COMPLETION); else ctx.write('$value');
        ctx.write(', ');
      }
      ctx.write('}}');
    }
    ctx.write('/>;\n');
  }
}
