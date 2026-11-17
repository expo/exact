# JavaScript type descriptors lose a record field named __proto__

**Status:** Closed
**Resolution:** A record field named `__proto__` is emitted as a computed key, so every JavaScript type descriptor holds it as an own field.
**Systems:** host/web-js/src/type_descriptors.rs, host/web-js/src/emit.rs, host/web-js/ts-data.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `type_json_defaults` emits record descriptors as JavaScript
object literals with quoted, noncomputed keys (`type_descriptors.rs:42-56`).
The compiler inserts them directly into `names.js`, including each source's
parameter and answer types (`emit.rs:647-657`, `:681-696`, `:711-718`).

Contract accepts a record field named `__proto__`. A descriptor such as
`{"__proto__":"s","label":"s"}` does not create that own property when
JavaScript evaluates it: the special object-literal syntax attempts to set
the object's prototype, and its string value is ignored. The resulting
record descriptor contains only `label`.

The data bridge enumerates those keys to map positional runner records to
named TypeScript objects (`ts-data.js:66-85`). It consequently maps the
first value, meant for `__proto__`, to `label` and drops the second value.
In the opposite direction, `written` rejects a valid answer containing the
missing field as outside the declared shape (`ts-data.js:190-204`). Such a
source works on the native record representation but fails on the JS target.
The bridge's existing special handling for `__proto__` cannot help because
the compiler has already removed the key before the bridge sees it.

A local probe used the production `type_descriptors.rs` with a valid
`PlanBuilder` record, evaluated its result as JavaScript, and ran the
unchanged `named` and `checked` functions extracted from `ts-data.js`:

```text
emitted: {"__proto__":"s","label":"s"}
fields: ["label"]
parameter for ["payload", "display"]: {"label":"payload"}
answer: `getRecord` answered outside its shape: field `__proto__` is not in the shape
```

The existing Contract executable also successfully compiled a source whose
answer shape has those two fields. Probe files are
`/tmp/exact-review-20261112/descriptor-proto.{contract,rs,mjs}`; the JavaScript
probe ran with pinned Bun 1.4.2. This covers the actual descriptor producer
and consumers, without building or launching a browser app.

Emit record descriptors with own data properties for every field name,
for example computed property keys, or parse serialized JSON at runtime.
Cover `__proto__` in parameter and answer records, nested records, module
source shapes and state inspection. Evaluate generated JavaScript in the
regression test; parsing the descriptor text as JSON alone misses the bug.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies. Closed
`20261110-web-persisted-inherited-name-breaks-boot.md` concerns inherited
lookups while reviving persisted scalar values; its own-key check does not
change compiler-emitted record descriptors.
