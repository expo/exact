// Whether a module can ask for a stream (LLP 1047.003 D5). A stream starts only where `fetch` reads its options'
// `exactStream`, so a page whose modules never spell that name is built without the stream reader, and its `fetch`
// refuses a name made at run time. A module spells it in its text, a comment included, or through an escape in a
// name, a string or a template; a string that holds it counts, since it may be parsed into options.
import { parseSync, Visitor } from 'rolldown/utils';

const NAME = 'exactStream';

/** Whether the module `id`, whose text is `code`, spells `exactStream`. */
export function namesStream(code, id) {
  if (/\.d\.[cm]?ts$/.test(id)) return false;
  if (code.includes(NAME)) return true;
  if (!code.includes('\\')) return false;
  if (id.endsWith('.json')) return JSON.stringify(JSON.parse(code)).includes(NAME);
  let found = false;
  new Visitor({
    Identifier: node => { found ||= node.name === NAME; },
    Literal: node => { found ||= typeof node.value === 'string' && node.value.includes(NAME); },
    TemplateElement: node => { found ||= !!node.value.cooked?.includes(NAME); },
  }).visit(parseSync(id, code).program);
  return found;
}
