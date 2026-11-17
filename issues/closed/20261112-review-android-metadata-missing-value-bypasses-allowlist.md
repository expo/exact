# Omitting a metadata value bypasses the Android manifest allowlist

**Status:** Closed
**Resolution:** A meta-data entry must be an object with a string name on the list and a string value equal to that name's, and both attributes are escaped when written.
**Systems:** scripts/android-manifest.mjs, js/bake/src/typescript.mjs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `manifestFragment` validates metadata by comparing
`METADATA.get(c.name)` with `c.value` (`android-manifest.mjs:173`). For an
unknown name with no value, both are `undefined`, so the comparison accepts
it. The next lines interpolate the unchecked name into XML without escaping.

The intended boundary admits only the exact name/value pairs in `METADATA`.
A package can instead add an arbitrary metadata name by omitting `value`.
Names containing XML delimiters also reach the attribute unchanged. The
allowlist therefore does not provide the constraint assumed by this writer.

## Reproduction

Copy `packages/fixtures/swatch` into a temporary app's
`node_modules/@fixture/swatch`, and add this to its `android/exact.json`:

```json
{"manifest":{"metadata":[{"name":"review.unknown"}]}}
```

Run `androidImplementations(dir, {swatch: '@fixture/swatch'})`, then pass
the result to `manifestFragment`. Both succeed and produce:

```xml
<application>
    <meta-data android:name="review.unknown" android:value="undefined" />
</application>
```

Verified with the actual package reader and manifest writer under Bun
1.4.2. Supplying `value: "true"` for that same unknown name correctly
refuses it; the allowed Google Pay pair succeeds. No APK was built or
installed. The probe is
`/tmp/exact-review-20261112/android-metadata-probe.mjs`.

Require a metadata object with string `name` and `value`, check that the
name exists in `METADATA`, then compare its required value. Escape both
attributes when writing XML. Cover missing fields, unknown names with and
without values, wrong value types, XML delimiters, and the allowed pair.

Searched open and closed filesystem issues and all 275 GitHub issues in
the review snapshot. No matching report was found. GitHub #181 concerns
Maven dependencies, which use a separate validator.
