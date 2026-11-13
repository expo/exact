# Native builds never refresh app.contract.d.ts, and the guide does not say which commands do

**Status:** Open
**Systems:** js/bake/src/lib.rs, scripts/exact.mjs (`refreshTypes`), host/web-js/build.mjs, docs/start-here.md
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`app.contract.d.ts` beside `app.ts` holds the types `app.ts` imports. Four things write it:
`contract build app.contract` and `pattern` (`refreshTypes`, `scripts/exact.mjs:787`, called at
`:726` and `:1238`), the web build when it type-checks a development build
(`host/web-js/build.mjs:329-334`), and the native bake in development mode only
(`js/bake/src/lib.rs:1118-1122`). That development mode is
the resident producer's (`js/bake/src/resident.rs:50`); every native build through Cargo bakes
with `BakeMode::Production` (`js/bake/src/lib.rs:259`, `:990`), so `bun exact.mjs ios`, `mac`,
`android`, `linux` and their `test` and `verify` builds check `app.ts` against fresh types in
their stage and leave the file beside it as it was. A builder's own `tsc`, or an editor, then
reports errors the build did not.

`docs/start-here.md:72-74` says to create the file once with `contract types` and that
`contract build` refreshes it. It does not say that `test`, `verify` and the native verbs do
not, or that the web build does.

## The failure

A builder at `8cfaf6775` added two sources to `app.ts` and the Contract; `bun exact.mjs test`
and `bun exact.mjs android` built and passed without regenerating `app.contract.d.ts`, and `tsc`
on `app.ts` failed until they ran `bun exact.mjs contract types app.contract > app.contract.d.ts`
by hand. Why their `test` (web by default, which does write the file) left it stale was not
determined; the Android half follows from the code above.

## Fix

Write the file beside `app.ts` from every development build, as the web build does: the native
one-shot bake knows whether it is a development build (`EXACT_UPDATE_TRUST`). And say in
`docs/start-here.md` that every build refreshes it, or name the commands that do.
