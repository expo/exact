# Test-server cleanup can miss unobserved descendants

**Status:** Open
**Systems:** scripts/app-server.mjs, scripts/agent-test.mjs, scripts/verify.mjs, docs/recipes/tests.md
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

Owned servers now restart empty before each test, including memory-only servers.
Direct `test` and `verify` no longer share the previous test's database. Remote
servers remain shared. Only `test` and `agent --test` share an ancestor's server;
`verify` refuses an answering local server, including an ancestor's.

Process ownership remains incomplete on POSIX and Windows. A descendant forked
before its parent is observed can escape the observed process set. POSIX cleanup
now refuses a nonempty group with no surviving observed member, retaining the
database, owner and port turn. Windows can still miss an unobserved descendant.
Neither platform yet establishes ownership of every descendant before execution.

## The failure

An admin app builder at `94b8671f7` with more than 70 tests over a server fixture: a fixture
write in one test leaked into the next. They added a test-only reset route to the server and a
`request` to it, then a reload, at the start of each test.

## Fix

Restart the owned server and clear its scratch database between tests. This is
implemented and covered on the Mac and web. Within one test, requests and reloads
keep that test's server data.

## Remaining

- An owned process supervisor on POSIX, kept alive through teardown, to retain
  authority over a worker forked during SIGTERM handling after the last observed
  member exits. The regression substitutes that handoff and verifies that cleanup
  fails naming the group and retains the database, owner and port turn.
- A Job Object on Windows, assigned before execution, to own descendants whose
  intermediate parent exits before observation. The process-table walk is not
  that ownership mechanism. The stubbed survivor is the child of a reused worker
  PID; it does not reproduce an intermediate parent absent from every observation.
  A PID can also be reused between creation-time validation and `taskkill`, killing
  an unrelated replacement; the Job Object must remove that window. A launcher
  continually replacing its workers can keep the leaf-first walk from reaching
  the launcher. The walk now fails at the shutdown deadline, retaining the
  database, owner and port turn, but a Job Object is needed to stop the whole
  owned tree despite respawning.
- A native Windows run covering an exiting launcher, CIM rows with no creation
  date, delayed process visibility, and database file locks. Mac regressions
  substitute the Windows process table and individual PID kills, including older unrelated
  orphans, children born during teardown, unrelated PID reuse and owned replacements
  reusing an observed PID before or during teardown;
  they do not qualify native Windows cleanup.
