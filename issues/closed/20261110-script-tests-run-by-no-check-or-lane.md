# 68 of the 125 `scripts/*.test.mjs` files are on no async-lane list, and two of them fail

**Status:** Closed
**Resolution:** `glueTests` globs `scripts/*.test.mjs` as it does the hosts' test files, so every script test runs in the glue step unless `LEFT_OUT` names it, and the tests that joined it no longer fail on assertions, fixtures or timeouts the code has moved past.
**Systems:** scripts/async.mjs (glueTests), scripts/caps.test.mjs, scripts/agent-test.test.mjs, .github/workflows/tests.yml
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`; the counts are those of `56e325b8`.

The async lane's glue step runs the files `glueTests` returns (`scripts/async.mjs:119`,
run at lines 225 and 226). The function globs the test files of `host/web`, `host/web-js`
and `modules/*/tests`, but names the `scripts/` ones one by one (line 123): 57 names. The
directory holds 125 test files, so 68 are on no list, and no other step of the lane names
one. The lane never runs a script test written without adding its name there.

What else can run them:

- The six blocking checks start no Bun test.
- `scripts/affected.mjs:124` makes every tracked test file a suite, so
  `bun scripts/affected.mjs --run` runs an unlisted file when a branch's change reaches it.
  That is advice run by hand; the lane's own selection keeps only what is on the list
  (`scripts/async.mjs:147`).
- The `tests` workflow's Linux job runs `bun test scripts` after the Cargo tests
  (`.github/workflows/tests.yml`). Nothing waits for it and it names no breaking commit.
  QUEUE.md ("The `tests` workflow is red on Linux for reasons older than it") records 25
  script tests failing there, most for a toolchain the runner lacks, so a failure in an
  unlisted file is one more line of a red run.

The unlisted files include `caps`, `deploy`, `verify`, `rust`, `feedback`, `evidence`,
`help`, `share`, `windows`, `agent-test`, and 29 of the `tamber-*` and `design-tool-*`
files. This prints all 68:

```sh
bun -e "const { glueTests } = await import('./scripts/async.mjs'); const on = new Set(glueTests('.')); console.log([...new Bun.Glob('scripts/*.test.mjs').scanSync('.')].filter((f) => !on.has('./' + f)).sort().join('\n'))"
```

## The failure

Two of the unlisted files fail on an assertion the code has moved past:

- `scripts/caps.test.mjs`, "web root switches a complete immutable graph; …". Line 623
  expects a direct write of a stream head to be refused with "stream heads require
  putHead". `scripts/origin.mjs:229` now says "stream heads and locks require the held
  stream lock". The test reports only `reader rounds: 0`, because every later step is
  skipped once its flag is false. With the current message in the assertion the case
  passes (22 reader rounds).
- `scripts/agent-test.test.mjs`, "authored media preferences apply before observations,
  override by name, and survive reload". It expects `line 28: prefer: unknown dark:
  expected`; `scripts/agent-prefer.mjs:52` now puts "did you mean `prefer
  prefers-color-scheme dark`?" before "expected".

Neither is among QUEUE.md's "Three script tests fail on main", which are in
`game/new-app.test.mjs` and `scripts/exact.test.mjs`.

Two cases of `scripts/deploy.test.mjs` also fail in a worktree whose `llp` is the symlink
AGENTS.md prescribes ("captured symlink llp has an absolute target that escapes the
captured source repositories"), and pass with the link removed.

## Verification

Counted with `glueTests` itself at `56e325b8` (301 files, 57 under `scripts/`) against the
directory (125). At `dd6f5dcd`, ran each of the 67 files then unlisted once with
`bun test scripts/<file>`: 62 files passed. Of the other five, the two above failed again
when run alone, at `dd6f5dcd` and at `56e325b8`, and `deploy.test.mjs` is as described. The
last two say nothing about the tree: `feedback.test.mjs` failed one case only because the
run set `EXACT_FEEDBACK_URL` to a closed local port so that nothing could be sent (the case
passes without it), and `package-libraries-link.test.mjs` needs native builds the machine
used could not make. Not run: `scripts/file-url.test.mjs`, added since.

## Direction

Glob `scripts/*.test.mjs` in `glueTests` as the hosts' files are, and name in `LEFT_OUT`,
with its reason, any file that cannot run in the lane. Correct the two assertions.

## Related

QUEUE.md, "Three script tests fail on main", lists other tests.
`issues/20261110-async-lane-results-are-not-on-origin.md` covers the lane's results not
reaching origin.
