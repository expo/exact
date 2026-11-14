import { expect, test } from "bun:test";
import { updates, declaredLists } from "./update.js";

// A keyed list whose entry is updated by the value a send carries, and tapped on the system's screen.
const listed = () => {
  const row = { sub: { list: { state: "listed", tap: { choices: { accept: { set: { status: "accepted" } } } } } } };
  const entry = { ref: "e1", row, fields: { id: "x", status: "new" } };
  const up = updates({ lists: new Map([["listed", [entry]]]), rows: () => [row], say() {}, now: () => 0, changed() {}, raise() {} });
  return { up, entry };
};

// The words are the app's values and the driver's text: a name Object.prototype holds is no key of the declaration.
test("a mapped value or a choice that names an inherited member is not one the declaration has", () => {
  const update = { key: "id", set: { status: { from: "word", map: { active: "on" } } } };
  for (const word of ["active", "constructor", "toString", "__proto__"]) {
    const { up, entry } = listed();
    expect([word, up.update("send", update, { id: "x", word }), entry.fields.status]).toEqual([word, null, word === "active" ? "on" : "new"]);
  }
  for (const choice of ["constructor", "toString", "__proto__"]) {
    const { up, entry } = listed();
    expect([choice, up.choose(entry, choice), entry.fields.status]).toEqual([choice, { error: `@e1 takes accept, not \`${choice}\`` }, "new"]);
  }
});

test("due entries, choices and updates move as the runner's app_list.rs moves them", () => {
  const list = { state: "alarms", key: "tag", scope: "app", due: {
    after: "duration", at: "at", into: "at", as: { after: { state: "countdown" }, at: { state: "scheduled" } }, fires: { state: "alerting" },
  }, tap: { choices: { stop: { retire: true }, snooze: { after: "snooze", set: { state: "countdown" } },
    act: { set: { state: "seen" }, event: "chosen", args: ["tag"] } } } };
  const row = { key: "fixture.schedule", sub: { list } }, other = { key: "other.schedule", sub: { list } };
  const changes = [], logs = [], events = [];
  let now = 100000;
  const lists = declaredLists({ rows: () => [row, other] });
  const up = updates({ lists, rows: () => [row, other], now: () => now, say: l => logs.push(l),
    changed: state => changes.push(state), raise: (...args) => events.push(args) });
  const fields = { tag: "tea", duration: 1000, snooze: 2000 };
  expect(up.listing(list, fields, row)).toBeNull();
  lists.set("fixture/alarms", [{ ref: 1, row, fields }]);
  const entry = () => lists.get("fixture/alarms")[0];
  expect(entry().fields).toEqual({ ...fields, at: 101000, state: "countdown", delivered: false });
  // An app-scoped list is its extension's: another extension's list of the same state is apart.
  expect(lists.state(other, "alarms")).toBe("other/alarms");
  expect(up.choose(entry(), "stop").error).toContain("not delivered yet");
  up.due(100999);
  expect(logs).toEqual([]);
  now = 101000;
  up.due(now);
  up.due(now);
  expect(logs).toEqual(["alarms @1: due"]);
  expect(changes).toEqual(["fixture/alarms"]);
  expect(entry().fields).toMatchObject({ state: "alerting", delivered: true });
  expect(up.choose(entry(), "snooze").choice).toBe("snooze");
  expect(entry().fields).toMatchObject({ state: "countdown", at: 103000, delivered: false });
  now = 102000;
  expect(up.update("pause", { state: "alarms", key: "tag", pause: "remaining", set: { state: "paused" } }, { tag: "tea" }, row)).toBeNull();
  expect(entry().fields).toMatchObject({ state: "paused", at: null, remaining: 1000 });
  now = 200000;
  expect(up.update("resume", { state: "alarms", key: "tag", resume: "remaining", set: { state: "countdown" } }, { tag: "tea" }, row)).toBeNull();
  expect(entry().fields).toMatchObject({ at: 201000, remaining: null, delivered: false });
  now = 201000;
  up.due(now);
  expect(up.choose(entry(), "act").event).toBe("chosen");
  expect(events).toHaveLength(1);
  expect(entry().fields.state).toBe("seen");
  expect(up.choose(entry(), "stop").choice).toBe("stop");
  expect(lists.get("fixture/alarms")).toEqual([]);
  expect(up.refusal({ state: "alarms", key: "tag" }, { tag: "tea" }, row)).toEqual({ error: "NotFoundError" });
});
