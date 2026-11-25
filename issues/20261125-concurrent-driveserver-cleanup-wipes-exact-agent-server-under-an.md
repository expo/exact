# Concurrent `driveServer` cleanup wipes `.exact/agent-server` under an active drive (orphaning its server), while the `starting` lock swallows `ENOENT` and fails open

**Status:** Open
**Systems:** Agent drive app server
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Check, then act)

Verification: verified by running.

At `9c0431fd5`, `driveServer` (`scripts/app-server.mjs:436-539`) has two interacting concurrency/error-handling defects in its `.exact/agent-server` lease lifecycle (`:473-477`, `:480-495`, `:529`):
1. **Destructive TOCTOU cleanup and orphaned server:** `stop()` (`:481-489`) calls `leave()` (`:447-472`) before `await stopGroup(group, server)` (`:484`), then calls `clean()` (`:473-477`) afterward. `clean()` checks only `if (files('server').length) return;` and ignores live `drive-*` leases and the `starting` lock. Whenever `db` is truthy (`:441` — true for every database-backed app server, even when `--storage` places `db.sqlite` outside `lease`), `:475` calls `rmSync(lease, { recursive: true, force: true })`, wiping `.exact/agent-server` underneath a concurrent Drive B. Worse, `writeFileSync(resolve(lease, \`server-${spawnedGroup.pid}\`), ...)` (`:529`) runs outside `failed()`, so an `ENOENT` there bypasses `await stop()`; on exit, `whenEnded` (`:480`) sees `launching = spawned && !spawnedGroup` as `false` and `files('server')` as empty, so it never kills `spawnedGroup`, leaving Drive B's supervisor and server permanently orphaned.
2. **Swallowed `ENOENT`, fail-open lock, and racy dead-owner unlink:** The `starting` lock loop (`:491-495`) wraps `writeFileSync(starting, String(process.pid), { flag: 'wx' })` in a bare `catch` that does not check `error.code === 'EEXIST'`. If `lease` was deleted by `clean()`, `ENOENT` is swallowed for 150 × 100 ms (15 s), after which execution falls through into `:496-537` with `locked === false` (failing open — as also happens on any > 15 s contention). On a dead owner, `:494` calls `rmSync(starting, { force: true })` without verifying the file still belongs to that dead PID, letting racing waiters unlink each other's freshly created `starting` lock.

### Reachable trigger
Run two overlapping `bun exact.mjs agent ...` (`scripts/agent.mjs:404`, e.g. `web` drives where `locking` is `false` at `agent.mjs:447`, or drives on different hosts/stores) or `scripts/fling.mjs:140` drives against an app with a database-backed server (`server.ts` or `commands.server`), such that Drive B enters `driveServer` while Drive A's `stop()` is awaiting `stopGroup(group, server)` (`:484`).

### Actual vs. expected
- **Actual:** Drive A's `clean()` deletes `.exact/agent-server` recursively even while Drive B's `drive-<pidB>` and `starting` files exist; if Drive B has already spawned its server (`:519-527`), `:529` throws `ENOENT` and orphans the running server with no lease record; if Drive B has not yet acquired `starting`, `:492-495` spins on swallowed `ENOENT` for 15 s and then proceeds unlocked.
- **Expected:** Per `scripts/app-server.mjs:431-434, 490`, overlapping drives share the server lease safely, only the last drive out stops and cleans up, and only one drive starts the server at a time.

### Verification and siblings
Verified by running a Bun probe against `driveServer` (`scripts/app-server.mjs`): while Drive A's `stop()` awaited `stopGroup` on a draining database-backed server, writing a live `drive-<ppid>` lease and `starting` lock into `.exact/agent-server` resulted in `stop()` deleting the entire `.exact/agent-server` directory (`existsSync(lease) === false`). The `:491-495` fail-open/dead-owner race and `:529` unguarded write were verified by reading the exact code paths.
- **Siblings checked:** `scripts/app-server.mjs:377-424` (`startServer` uses `takePort` from `scripts/port-lock.mjs` and non-recursive `rmdirSync(runDir)` at `:414`, safe); `host/web/build-lock.mjs:89-100` (`takeLock` uses OS file locks, never unlinks the lock, fails closed); `host/apple/touches.mjs:166-187`, `scripts/filesystem.mjs:289-315` (`claimBuild`), and `scripts/forma-relay-server.mjs:539-544` all check `EEXIST`, fail closed on timeout, and serialize or verify token ownership before removing stale lock files.

### Suggested fix and regression test
- Serialize `stop()` cleanup and `driveServer` startup under a persistent lock (e.g. `host/web/build-lock.mjs` `takeLock` outside `lease`), or in `clean()` re-check live `files('drive')`, `files('server')`, and `starting`, and delete only specific files (`db.sqlite*`, `log`, `ended-*`) followed by non-recursive `rmdirSync(lease)` instead of `rmSync(lease, { recursive: true, force: true })`.
- Guard `:529` (`writeFileSync` of `server-<pid>`) inside `try { ... } catch (e) { await failed(e); }` (and in `failed()` / `whenEnded`, kill `spawnedGroup` directly when no `server-<pid>` record exists).
- In `:491-495`, rethrow non-`EEXIST` errors (or re-`mkdirSync(lease)` if needed), verify owner token / serialize stale-lock takeover (as `host/apple/touches.mjs:179-184` or `scripts/filesystem.mjs:300-309` does), and throw if `!locked` after timeout.
- Add a regression test in `scripts/app-server.test.mjs` verifying that `stop()` does not wipe `.exact/agent-server` when another live `drive-*` or `starting` file is present, that a write failure at `server-<pid>` still kills the spawned server, and that `starting` lock failures fail closed.

### Siblings checked

scripts/app-server.mjs:377-424 (startServer uses takePort + rmdirSync); host/web/build-lock.mjs:89-100, host/apple/touches.mjs:166-187, scripts/filesystem.mjs:289-315, scripts/forma-relay-server.mjs:539-544 (all check EEXIST, fail closed, and avoid racing unlinks)
