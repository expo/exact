// `namesStream` (LLP 1047.003 D5): the name spelled as written or through an escape in a name, a string, a template
// or a JSON module; a name made at run time, or one only a declaration file holds, is not.
import { test, expect } from 'bun:test';
import { namesStream } from './stream-names.mjs';

test('a module names exactStream as written or through an escape, and not by a name made at run time', () => {
  const spelled = [
    ['a.ts', 'fetch(u, { exactStream: f })'],
    ['a.ts', 'fetch(u, { exact\\u0053tream: f })'],
    ['a.ts', 'o.exact\\u{53}tream = f'],
    ['a.ts', 'fetch(u, { "exact\\x53tream": f })'],
    ['a.ts', 'fetch(u, { ["\\exactStream"]: f })'],
    ['a.tsx', 'const x = <i title="\\n" />; const k = `{"exact\\u0053tream":1}`'],
    ['a.json', '{"exact\\u0053tream": 1}'],
  ];
  for (const [id, code] of spelled) expect(namesStream(code, id), code).toBe(true);
  const unspelled = [
    ['a.ts', 'fetch(u, { ["exact" + "Stream"]: f }); const s = "\\n"'],
    ['a.ts', 'fetch(u, { [key]: f })'],
    ['a.d.ts', 'export declare const exactStream: boolean'],
    ['a.json', '{"a": "\\n"}'],
  ];
  for (const [id, code] of unspelled) expect(namesStream(code, id), code).toBe(false);
});
