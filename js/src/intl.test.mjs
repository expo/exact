// js/src/intl.js's compact formatter, on an engine that ignores `notation`
// as Apple's Hermes does, against this runtime's own ICU compact. The
// prelude runs in a context of its own whose Intl.NumberFormat drops
// `notation` and `compactDisplay`; ICU's, outside it, is the reference.
import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { createContext, runInContext } from 'node:vm';

const source = readFileSync(new URL('./intl.js', import.meta.url), 'utf8');

function withoutCompact() {
  const context = createContext({ __exact_runtime_line: () => {} });
  runInContext(`(() => {
    const Engine = Intl.NumberFormat;
    function NumberFormat(locales, options) {
      const kept = options == null ? options : { ...options };
      if (kept) { delete kept.notation; delete kept.compactDisplay; }
      return Reflect.construct(Engine, [locales, kept], new.target || NumberFormat);
    }
    NumberFormat.prototype = Engine.prototype;
    NumberFormat.supportedLocalesOf = Engine.supportedLocalesOf;
    Intl.NumberFormat = NumberFormat;
  })();`, context);
  runInContext(source, context);
  return (locale, options, value) => runInContext('(l, o, v) => new Intl.NumberFormat(l, o).format(v)', context)(locale, options, value);
}

test('compact significant digits keep the minimum and drop the zeros past it', () => {
  const format = withoutCompact();
  const values = [0, -0, 5, 0.5, 0.01234, 0.0012, 1, 12, 999, 1000, 1200, 1234, 12345, 99999, 999999, 1e6, 1234567, -1234, 1.5e13];
  const options = [{ minimumSignificantDigits: 1 }, { minimumSignificantDigits: 3 }, { minimumSignificantDigits: 5 },
    { minimumSignificantDigits: 2, maximumSignificantDigits: 4 }, { minimumSignificantDigits: 3, maximumSignificantDigits: 3 },
    { minimumSignificantDigits: 2, maximumSignificantDigits: 2 }];
  const wrong = [];
  for (const locale of ['en-US', 'de-DE', 'ja-JP', 'fr-FR'])
    for (const o of options) for (const v of values) {
      const compact = { notation: 'compact', ...o }, got = format(locale, compact, v), icu = new Intl.NumberFormat(locale, compact).format(v);
      if (got !== icu) wrong.push(`${locale} ${JSON.stringify(o)} ${Object.is(v, -0) ? '-0' : v}: ${got}, ICU ${icu}`);
    }
  expect(wrong).toEqual([]);
});
