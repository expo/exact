import { expect, test } from "bun:test";
import { permissions } from "./permissions.js";

test("required permissions share states, rank denials first and ask required prompts in order", () => {
  const own = { name: "alarm", status: "status", field: "state", requires: { web: ["first", "second"] } };
  const first = { name: "first", status: "status", field: "nested.first", initial: "unknown" };
  const second = { name: "second", status: "status", field: "nested.second" };
  const row = { key: "fixture.schedule", sub: { permission: own, permissionList: [own, first, second] } };
  const shared = { key: "other.status", sub: { permission: first } };
  const states = new Map([["alarm", "granted"]]);
  const p = permissions(() => [row, shared], states);
  expect(p.stateOf("alarm")).toBe("prompt");
  expect(p.permissionOf(row, {}).name).toBe("second");
  states.set("second", "granted");
  expect(p.stateOf("alarm")).toBe("unknown");
  expect(p.permissionOf(row, {}).name).toBe("alarm");
  states.set("first", "denied");
  expect(p.stateOf("alarm")).toBe("denied");
  expect(p.stateOf(shared.sub.permission.name)).toBe("denied");
  states.set("first", "prompt");
  states.set("alarm", "denied");
  expect(p.permissionOf(row, {}).name).toBe("alarm");
  states.set("alarm", "prompt");
  expect(p.permissionOf(row, {}).name).toBe("first");
  const status = { key: "fixture.status", sub: row.sub };
  expect(p.statusValue(status, { nested: {} })).toEqual({ state: "prompt", nested: { first: "prompt", second: "granted" } });
  expect(p.statusValue(status, {})).toEqual({ state: "prompt" });
  own.requires.web = [];
  own.requires.android = ["first"];
  states.set("alarm", "granted");
  expect(p.stateOf("alarm")).toBe("granted");
});

test("dependencies declared across interfaces cannot recurse through a cycle", () => {
  const a = { name: "a", requires: { web: ["b"] } }, b = { name: "b", requires: { web: ["a"] } };
  const rows = [a, b].map(permission => ({ sub: { permission } }));
  const p = permissions(() => rows, new Map([["a", "granted"], ["b", "denied"]]));
  expect(p.stateOf("a")).toBe("denied");
  expect(p.stateOf("b")).toBe("denied");
});
