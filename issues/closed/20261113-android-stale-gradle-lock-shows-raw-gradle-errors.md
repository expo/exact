# An Android build whose Gradle lock no longer matches its graph fails with Gradle's errors and no advice

**Status:** Closed
**Resolution:** Locked Android builds preserve Gradle output and name the public --write-locks command for dependency-lock or verification failures.
**Systems:** scripts/agent-android.mjs (`mavenGraph`, `buildApk`, `runGradle`), scripts/help.mjs (`android`)
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

An app whose modules name Maven libraries keeps `gradle.lockfile` and `gradle-verification.xml`
beside its other locks. `mavenGraph` (`scripts/agent-android.mjs:1036-1050`) runs Gradle with
`--dependency-verification=strict` and the lock. The graph they cover also holds the reader's own
AndroidX, which changes with what the app uses: a plan with a date or time control adds Compose
Material 3 (`-Pcompose=1`, `:964`). When the graph changes, Gradle refuses with its own errors,
printed straight to the terminal (`runGradle`, `stdio: 'inherit'`, `:756`), and `buildApk`
returns null with nothing added (`:983`). The command that rewrites the locks
(`androidLockCommand`, `:995`) is named only when the files are missing (`mavenLocksPresent`,
`:1024-1028`), and `android --help` does not list `--write-locks` (it is internal,
`scripts/help.mjs:332-334`).

## The failure

At `8cfaf6775`: adding an `input type=date` to an app that already had `gradle.lockfile` and
`gradle-verification.xml` (written for another library) pulled Compose Material 3 for the picker.
The build then failed with "Dependency verification failed" for `savedstate-compose` and
"not part of the dependency lock state", and nothing said the locks were stale or how to rewrite
them.

## Fix

When Gradle fails under a lock and its output names dependency verification or the lock state,
say in one line that the app's Gradle locks no longer match its graph and that
`androidLockCommand(app)` rewrites them (to be reviewed and committed). List `--write-locks` in
`android --help`, since an app author needs it whenever the graph changes.
