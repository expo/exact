# Photos fixture panics on valid JSON options that are not an object

**Status:** Open
**Systems:** Photos fixture, Rust data source, image codec validation
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, entering `[]`, `123`, or `"text"` in the Photos fixture's
Options field and pressing Transform or Compress reaches a Rust panic
before any storage request. The fixture should report invalid options
through its existing error path so the image-codec drive can continue.

`apps/photos-fixture/app.contract:33-38` sends the field's text unchanged
to `editImage`. In `apps/photos-fixture/data/src/lib.rs:60-65`, JSON syntax
is validated but the top-level type is not. Assigning
`record["destination"]` at line 78 then panics on arrays and non-null
primitives. Malformed JSON already returns `DataError`; these valid JSON
values should receive the same kind of refusal.

Verified by calling the actual `photos_fixture_data::Fixture::answer`
with `editImage("app:/tmp/input.jpg", "transform", options)` and catching
the panic in the probe. No image or storage operation ran:

```text
options="{}" result=request
options="null" result=request
options="[]" result=panic
options="123" result=panic
options="\"text\"" result=panic
options="invalid" result=reported error
```

Probe: `/tmp/exact-review-20261112/photos-options-probe.rs`, linked against
the existing workspace libraries with `rustc`; the device app was not
launched. Require an object before changing `self.edits` or `self.steps`.
Decide explicitly whether `null` means empty options or is refused, and
cover arrays, primitives, malformed JSON, and a valid object.

Checked all open and closed filesystem issues and the full 275-issue
GitHub snapshot. The closed feedback JSON-null and Apple module NaN
issues concern separate request boundaries; no Photos fixture report
covers this failure.
