// Persisted state's first value on the web JS target. Run alone, `bun test ./host/web-js/persist.test.js`: it loads
// rt.js under the same stand-ins as store.test.js.
import { test, expect, mock } from "bun:test";

mock.module("./navigation.js", () => import("../web/navigation.js"));
mock.module("./prompt-glue.js", () => ({}));
mock.module("./alert-message.js", () => ({}));
mock.module("./menu-rows.js", () => ({}));
mock.module("./names.js", () => ({ sourceTypes: {} }));
const { Kept } = await import("./rt.js");
const { stored } = await import("./persist.js");

// Only the runner's three spellings revive as a number (runner/src/runner/persist.rs); a stored name that
// Object.prototype holds is a value that does not fit, and the initial value stands.
test("a stored string revives as a number only when it spells NaN, Infinity or -Infinity", () => {
  const saved = Object.getOwnPropertyDescriptor(globalThis, "localStorage"), disk = new Map();
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: { getItem: key => disk.get(key) ?? null } });
  const first = (text, t) => { disk.set(Kept("state") + "q", text); return stored("q", 7, t); };
  try {
    expect(["\"NaN\"", "\"Infinity\"", "\"-Infinity\"", "-0"].map(text => first(text, "n"))).toEqual([NaN, Infinity, -Infinity, -0]);
    for (const name of ["constructor", "toString", "valueOf", "hasOwnProperty", "__proto__"]) {
      expect([name, first(JSON.stringify(name), "n"), first(JSON.stringify(["Infinity", name]), ["[", "n"])]).toEqual([name, 7, 7]);
    }
  } finally {
    if (saved) Object.defineProperty(globalThis, "localStorage", saved); else delete globalThis.localStorage;
  }
});
