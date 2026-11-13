# Source compatibility signatures discard enum membership

**Status:** Open
**Systems:** bake/src/receipt.rs, scripts/app.mjs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, the artifact graph treats every string enum
as an unrestricted string. A plan update can therefore pass the source
compatibility check against an installed implementation whose answers the
new plan rejects.

`shape` handles options, lists and records, then writes only the type kind
for everything else (`bake/src/receipt.rs:473-491`). An enum is a
`TypeKind::String` row whose fields contain the allowed strings. Those
fields are omitted. Both the frozen cohort and the candidate graph use
this representation (`receipt.rs:523-534`), so
`classifyArtifacts` finds equal source shapes (`scripts/app.mjs:1368-1369`).

Verified with the current `target/debug/contract` and `exact-bake` binaries:

```contract
shape Result
  state: enum<"old", "common">

component App
  resource result = read() as shape Result
  view
    text result.state
```

Compile this with `contract build old.contract -o old.plan`. Change the
vocabulary to `enum<"new", "common">` and compile `new.plan`. Running
`exact-bake graph <plan> <compat.json> aarch64-apple-darwin` on each gives
the same `read` signature:

```json
{"params":[],"result":{"fields":[{"name":"state","type":"String"}],"kind":"Record"}}
```

A local probe passed these actual graphs to the unchanged
`classifyArtifacts`, using identical platform capabilities and an installed
source-bound cohort. It returned
`{"binary":false,"bundle":true,"missing":[],"warnings":[]}`. A separate
probe linked to the current `exact-plan` library decoded both plans and
checked the installed source's `{state:"old"}` answer with `Value::conforms`:
`old: true`, `new: false`. The runner enforces that check when admitting a
reply (`runner/src/runner/reply.rs:440`), so the update loses a previously
valid resource answer. Probe sources and generated plans are in
`/tmp/exact-review-20261112/enum-graph/`; no update was published.

This concerns plans retaining a `sources` requirement and an installed
native implementation. A paired replaceable Rust or TypeScript module
removes that requirement and is outside this reproduction.

Include the enum's members in semantic source shapes, nested as well as
top-level. Keep unrelated plan row numbers out of the signature. Refuse
changed vocabularies against old receipts that do not record membership,
or version the receipt so old data cannot be mistaken for an unrestricted
string. Validate changed result and parameter vocabularies, plain-string
to enum changes, and harmless unrelated declaration insertion through the
shared classifier.

Searched open and closed filesystem issues and all 275 cached GitHub
issue bodies. The closed `20261110-types-summary-enum-as-string` concerns
TypeScript summary printing; it does not cover compatibility receipts or
update admission. No existing issue covers this omission.
