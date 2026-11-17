# Edits to extensionless JavaScript imports pass the build freshness check

**Status:** Closed
**Resolution:** `moduleGraph` follows a relative import to every file the bundler may resolve it to, JavaScript extensions and index files included, and scans `.jsx` for imports, so an edit to such a file makes the build stale.
**Systems:** scripts/agent-launch.mjs, scripts/agent-build.mjs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, an app can import `./helper` from `app.ts`, with its code in
`helper.js`. The production bundler resolves and includes that file, but
`moduleGraph` does not: its candidate list includes TypeScript and JSON
extensions, but neither `.js` nor `/index.js`
(`scripts/agent-launch.mjs:1027`). `notBuildInput` then excludes the JavaScript
file as unreachable. Editing it leaves the web build's freshness result
empty, so a drive can qualify the old code without rebuilding.

The same omission applies to `import './helpers'` resolved to
`helpers/index.js`. An explicit `./helper.js` import is tracked correctly.
Native plan selection also consumes this graph; the reproduction below
qualifies the web freshness result.

A temporary probe imported the actual `moduleGraph`, `webInputDigests` and
`webChanges` functions. For each import spelling it bundled a small data
module with the checkout's pinned Rolldown and production `dataBundle`
configuration, wrote the build marker from `webInputDigests`, edited the
dependency, and compared both the freshness result and a new bundle:

| Import | Resolved file | Bundle changed | Reported changed app inputs |
| --- | --- | --- | --- |
| `./helper` | `helper.js` | yes | none |
| `./helpers` | `helpers/index.js` | yes | none |
| `./helper.js` | `helper.js` | yes | `helper.js` |

Run with Bun 1.4.2:
`/tmp/exact-review-20261112/extensionless-js-freshness.mjs`.
The probe uses actual bundling and freshness code; it does not launch a
browser or native app.

Use the compiler's resolved input graph where possible. If this scanner is
retained, cover the JavaScript and directory resolutions the bundler accepts
and conservatively keep inputs when resolution is uncertain. Test an edit
to each resolved dependency through the build marker and freshness check,
including extensionless JavaScript, directory imports and transitive imports.

Checked open and closed filesystem issues and all 275 cached GitHub issues.
The closed source-graph issue concerns strings tables in the Contract
compiler; the closed deep-source issue concerns the serving walk's former
depth limit. Neither covers the JavaScript resolution mismatch here.
