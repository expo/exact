import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

test("navigation's concrete ARIA roles match the schema", () => {
  const schema = JSON.parse(readFileSync(new URL("../../../kernel/tables/schema.json", import.meta.url), "utf8"));
  const source = readFileSync(new URL("../navigation/lists.js", import.meta.url), "utf8");
  const roles = source.match(/const concreteRole = \/\^\(\?:([^)]*)\)\$\/i/)[1].split("|");
  expect(roles.toSorted()).toEqual(schema.ariaRoles.toSorted());
});
