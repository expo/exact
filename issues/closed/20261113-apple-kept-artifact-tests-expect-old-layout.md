# exact-apple's kept-artifact tests fail on main

**Status:** Closed
**Resolution:** The code was right and both tests were not: the kept-crates fixture now writes dep-info as rustc does (the `.d` file names itself first, which `take` checks), and the ownership test counts one Swift compile per destination and trust, as `appleArtifacts` makes them.
**Systems:** host/apple (modules.mjs, crates.mjs), host/apple/tests/it/inherited.rs
**Severity:** P3
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

Two `async lane:` tests of `exact-apple`'s `inherited.rs` fail:

- `inherited::kept_registry_crates_are_one_target_directory_at_a_time` (line 736 at 69668888a): a second target directory that takes the kept crates gets none of their units: `[]`, not `["over-plain-0000000000000002", "plain-0000000000000001"]`.
- `inherited::apple_artifacts_own_paths_locks_identity_and_failed_placement` (line 556): "one Swift compile per destination, whatever the app, composition, trust or product" finds 6 Swift directories where it expects 3.

## How to reproduce

```
cargo test -p exact-apple --test it -- --ignored kept_registry_crates_are_one_target_directory_at_a_time apple_artifacts_own_paths_locks_identity_and_failed_placement
```

Both fail at 69668888a on the async lane's Mac, at f932ba81a on a second Mac, and in the lane's run at afde7b25e.
