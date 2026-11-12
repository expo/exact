import { expect, test } from "bun:test";
import { updates } from "./update.js";

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
