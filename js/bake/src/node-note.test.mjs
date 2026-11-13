import { expect, test } from 'bun:test';
import { nodeNote } from './typescript.mjs';

// The checker's TS2591 for a Node built-in suggests @types/node, which changes nothing; the note says why it fails
// (expo/exact-build#183).
test("a type check that cannot find a Node built-in says it is Node's, not that types are missing", () => {
  const ts2591 = "app.ts(1,30): error TS2591: Cannot find module 'node:fs'. Do you need to install type definitions for node? Try `npm i --save-dev @types/node` and then add 'node' to the types field in your tsconfig.";
  expect(nodeNote(ts2591)).toContain("'node:fs' is Node's built-in: a data module runs on Hermes and in the browser");
  expect(nodeNote(`${ts2591}\napp.ts(2,20): error TS2307: Cannot find module 'path' or its corresponding type declarations.`)).toContain("'node:fs', 'path' are Node's built-ins:");
  expect(nodeNote("app.ts(3,1): error TS2580: Cannot find name 'process'. Do you need to install type definitions for node?")).toContain("Node's globals are");
  // Another package that is not installed, or any other error, gets no note.
  expect(nodeNote("app.ts(1,20): error TS2307: Cannot find module 'left-pad' or its corresponding type declarations.")).toBe('');
  expect(nodeNote("app.ts(4,7): error TS2322: Type 'string' is not assignable to type 'number'.")).toBe('');
});
