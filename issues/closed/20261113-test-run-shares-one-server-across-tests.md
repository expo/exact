# A test cannot start on an empty server database: `test web` runs one server for the whole run

**Status:** Closed
**Resolution:** test --help, the tests recipe and the REST recipe say the server's database is the run's, shared by its tests, and the REST recipe shows the reset route and the test lines that empty it first; a database per test would change LLP 1156.001 D5 and is not built.
**Systems:** scripts/app-cli.mjs, scripts/verify.mjs (`withAppServer`), docs/recipes/tests.md
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`bun exact.mjs test` wraps the whole run in one `withAppServer` call
(`scripts/app-cli.mjs:479-489`), which starts `server.ts` on an empty
`.exact/test/db.sqlite` once and stops it after the last test (`scripts/verify.mjs:386-402`).
Each test launches the app with empty client storage, but the server's database is shared.

That is the documented design: "The database is empty when a run starts and shared by its
tests … a test that needs no earlier rows makes its own" (`docs/recipes/rest-backend.md:53`).
The advice covers rows a test needs, not rows an earlier test wrote, changed or deleted: a test
that asserts a list's count or an empty state depends on which tests ran before it. No test line
resets the server, so each app that needs isolation writes its own reset route into its server.
`test --help` and `docs/recipes/tests.md` do not say the server is per run.

## The failure

An admin app builder at `94b8671f7` with more than 70 tests over a server fixture: a fixture
write in one test leaked into the next. They added a test-only reset route to the server and a
`request` to it, then a reload, at the start of each test.

## Fix

Offer an empty database per test: a test header or flag that starts that test on a fresh copy of
the database file (stopping and restarting the server around it), or a `reset server` line. At
least, say in `test --help` and the tests recipe that the server is per run, with the reset
pattern.
