# Two apps whose crates share a name and a CARGO_TARGET_DIR make each other's Apple builds stale and busy

**Status:** Closed
**Resolution:** An app outside exact2's workspace records its crate names in a shared `CARGO_TARGET_DIR` at its first build, and another app whose crates have the same names is refused there, naming both directories, while the first still exists.
**Systems:** scripts/app.mjs (`resolveApp`, `assertOwnTarget`, `appleCargoClaims`, `claimBuildOutput`), scripts/agent-launch.mjs (`receiptChanges`), host/apple/build.mjs
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

An app's target directory is `CARGO_TARGET_DIR` when it is set (`scripts/app.mjs:511`).
`assertOwnTarget` (`:431-436`) refuses only a target inside another checkout of the same git
repository; two apps, or two copies of one app, pointed at one directory are accepted. Most of
an Apple build's outputs are then kept apart by the app's source and id
(`appleArtifacts`, `host/apple/build.mjs:156`), but two things are keyed by the crate's name alone:

- the claim on the Cargo build, `<target>/.apple-cargo-locks/<triple>/lib<crate>.lock`
  (`appleCargoClaims`, `scripts/app.mjs:1217-1218`), taken with `claimBuildOutput`, which
  refuses a held claim with `Apple build busy for <id>: <the holder's claim>`;
- Cargo's own uplifted outputs, among them the dep-info `<target>/<triple>/<profile>/lib<crate>.d`,
  which `receiptChanges` reads to list the build's inputs (`scripts/agent-launch.mjs:1129-1131`).

Since efeb4dbe a generated workspace names its crates from the name the app's `exact.mjs`
passes, not from its folder, so every checkout or copy of an app has the same crate names. Two
such apps sharing a target overwrite each other's `lib<crate>.d`: after B builds, A's freshness
check reads B's dep-info, lists B's files as A's changed inputs, and rebuilds A; and while one
builds, the other's build is refused as busy.

The mechanism above is read from the code and fits every detail of the report; it was not
reproduced (it needs two Apple builds).

## The failure

macOS, at `9e53578f2`: with two apps sharing one `CARGO_TARGET_DIR`, `agent macos` for app A
said

```
the build is older than its sources (generated:<A>-apple:aarch64-apple-darwin/module.rs, <B>/package.json changed)
```

after app B built, rebuilt every time, and then failed with
`Apple build busy for <A's id>: {"pid":…,"app":"<B's id>",…}` while B was building. A's own
sources had not changed.

## Fix

Give each app its own subdirectory under a shared `CARGO_TARGET_DIR` (for example by
`appSourceKey`, as `target/clients/` already is), or refuse a target another app's source
recorded, naming both apps. At the least, let `receiptChanges` ignore a dep-info whose recorded
inputs lie in another app's directory, and say in the busy refusal that the holder is a
different app sharing the target.
