// The prelude's reply (js/src/binary.js `keptReply`) keeps a frozen row's JSON only in the native runtime
// (its byte door) with the prototypes hardening froze: the web's module realm is neither, so each reply is a
// plain JSON.stringify there, whatever a prototype or a Proxy does later.
import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { createContext, runInContext } from 'node:vm';

const binary = readFileSync(new URL('../../js/src/binary.js', import.meta.url), 'utf8');

function realm(harden) {
  const context = createContext({});
  runInContext(binary, context);
  runInContext(`globalThis.later = false;
    // A Proxy whose trap answers no toJSON at first and one later: a kept row holding it would go stale.
    globalThis.turning = new Proxy(Object.freeze({ k: 'still' }), { get: (t, k, r) => k === 'toJSON'
      ? (later ? () => ({ k: 'turned' }) : undefined) : Reflect.get(t, k, r) });
    globalThis.rows = Object.freeze(Array.from({ length: 20 }, (_, i) => Object.freeze({ id: 'r' + i,
      holes: Object.freeze([, 1]), inner: Object.freeze([i === 3 ? turning : Object.freeze({ k: 'plain' })]) })));`, context);
  if (harden) runInContext('Object.freeze(Object.prototype); Object.freeze(Array.prototype);', context);
  return context;
}
const reply = (context) => runInContext('__exact_binary.keptReply({ tag: 0, value: rows }, null)', context);
const plain = (context) => runInContext('JSON.stringify({ tag: 0, value: rows })', context);

test('native replies do not probe ordinary records or arrays for the String brand', () => {
  const context = createContext({ __exact_bytes() {} });
  runInContext(`globalThis.probes = 0;
    const valueOf = String.prototype.valueOf;
    String.prototype.valueOf = function () { probes++; return valueOf.call(this); };`, context);
  runInContext(binary, context);
  runInContext(`Object.freeze(Object.prototype); Object.freeze(Array.prototype);
    globalThis.rows = [{ id: 'r1', inner: [{ k: 'plain' }] }, Object.create(null)];`, context);
  expect(reply(context)).toBe(plain(context));
  expect(runInContext('probes', context)).toBe(0);
});

test('native replies preserve boxed primitives and do not inspect Proxy descriptors', () => {
  const context = createContext({ __exact_bytes() {} });
  runInContext(binary, context);
  runInContext(`Object.freeze(Object.prototype); Object.freeze(Array.prototype);
    const changed = new String('ordinary');
    Object.setPrototypeOf(changed, null);
    changed.toString = () => '\\u0000' + '1';
    const proxy = Proxy.revocable({ text: 'proxy' }, {
      getOwnPropertyDescriptor(target, key) {
        if (key === 'length') throw new Error('unexpected length descriptor');
        return Reflect.getOwnPropertyDescriptor(target, key);
      }
    }).proxy;
    const lookalike = { length: 1, [Symbol.toStringTag]: 'String',
      toString() { throw new Error('not a String'); } };
    globalThis.rows = [Object.freeze({ kept: true }), changed, proxy, lookalike,
      new Number(3), new Boolean(false)];`, context);
  expect(reply(context)).toBe(plain(context));
  expect(JSON.parse(reply(context)).value[1]).toBe('\u00001');
  runInContext('rows.push(Object(1n));', context);
  expect(() => reply(context)).toThrow(/BigInt/);
  expect(() => plain(context)).toThrow(/BigInt/);
});

test('a realm whose prototypes are not frozen keeps no row, so a later prototype change shows', () => {
  const context = realm(false);
  expect(reply(context)).toContain('"holes":[null,1]');
  runInContext('Array.prototype[0] = "filled";', context);
  expect(reply(context)).toBe(plain(context));
  expect(reply(context)).toContain('"holes":["filled",1]');
});

test('a hardened realm without the native door keeps no row either, so a Proxy that turns shows', () => {
  const context = realm(true);
  expect(reply(context)).toBe(plain(context));
  runInContext('later = true;', context);
  expect(reply(context)).toBe(plain(context));
  expect(reply(context)).toContain('"turned"');
});
