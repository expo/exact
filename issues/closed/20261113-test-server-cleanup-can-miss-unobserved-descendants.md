# Test-server cleanup can miss unobserved descendants

**Status:** Closed
**Resolution:** The app server runs under a supervisor that owns its tree before it runs: a process group (and on Linux a subreaper) on POSIX, a kill-on-close job on Windows; the native Windows run is in QUEUE.md.
**Systems:** scripts/app-server.mjs, scripts/processes.mjs
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## The failure

An admin app builder at `94b8671f7` with more than 70 tests over a server fixture: a fixture
write in one test leaked into the next. That part was fixed first: each test starts on an
empty server.

## What was wrong

`test`, `verify` and `agent` stopped the app server they started by observing its processes
after the fact: on POSIX the members of its process group, compared with an earlier listing;
on Windows its descendants by parent PID and creation time, each ended with `taskkill /PID`.
A worker forked during SIGTERM handling after the last observed member exited, a descendant
whose parent exited before any observation, or a launcher that kept replacing its workers
could escape, and on Windows a PID could be reused between the check and `taskkill`.

## The fix

The server runs under a supervisor (`scripts/processes.mjs` `supervise`) started in a
session of its own, so no signal to its owner's process group reaches it, and it takes
authority over every process the server will start before the server runs:

- POSIX: the supervisor's child, the keeper, leads a new process group, starts the server
  in it, then returns to the supervisor's group. Every descendant is a member from its
  fork, and the group's id is the keeper's pid, which no other process can take while the
  keeper runs and then while the supervisor holds it unreaped. On Linux the keeper is also
  the subreaper of the server's tree, so a descendant that leaves the group (`setsid`)
  comes back to it when its parent exits and is ended with the group. macOS has no
  subreaper: such a descendant is left running, and fails the run while it holds the
  server's port. Once the keeper has ended everything it records so beside the run's owner
  file or the drive lease, and whoever stops the server reads that record; on Linux one
  without it is refused.
- Windows: the supervisor creates a job with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, joins
  it, then starts the server, so every descendant is a member and none can break away.
  Stopping terminates the job and waits until it has no active process; a drive finds
  another drive's server by the job its record names.

An owner's end, however it ends, closes the supervisor's stdin and so ends a `test`
server; a drive's server outlives the drive that started it. A stop that cannot end
everything fails naming the group or job, keeping the database, owner file and port turn.
The POSIX behaviour is tested with real processes on macOS and Linux, including an owner
whose whole process group is killed. The Windows job calls are tested only through a
substituted job table: no Windows machine has run them (`QUEUE.md`).
