# exact-apple's development plan test names an ExactApp member that is gone

**Status:** Open
**Systems:** host/apple (ExactKit), host/apple/tests/it/development.rs
**Severity:** P3
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

`exact-apple`'s `development::local_development_plan_loads_the_pair_and_retries_partial_writes`, an `async lane:` test, compiles a Swift driver that no longer compiles:

```
error: value of type 'ExactApp' has no member 'generationPending'
143 |   last = Self.seen(revision, last: last, apply: { apply(to: app) }, pending: { app.generationPending })
```

## How to reproduce

```
cargo test -p exact-apple --test it -- --ignored local_development_plan_loads_the_pair_and_retries_partial_writes
```

Fails at 69668888a on the async lane's Mac, at f932ba81a on a second Mac, and in the lane's run at afde7b25e.
