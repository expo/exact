# compressImage's generated type is wider than the TypeScript dispatcher test's provider

**Status:** Open
**Systems:** contract (TypeScript types), contract/cli/tests/it/typescript.rs
**Severity:** P3
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

`contract`'s `typescript::generated_signatures_check_real_sync_and_async_providers_and_the_dispatcher`, an `async lane:` test, fails `tsc`:

```
app.ts(50,9): error TS2322: Type 'CompressedImage' is not assignable to type '{ path: string; type: "image/jpeg"; size: number; width: number; height: number; }'.
  Types of property 'type' are incompatible.
    Type '"image/jpeg" | "image/png"' is not assignable to type '"image/jpeg"'.
```

The generated `CompressedImage` now allows `image/png`; the test's provider still says only `image/jpeg`. Decide which is right and change the other.

## How to reproduce

```
cargo test -p contract --test it typescript:: -- --ignored generated_signatures_check_real_sync_and_async_providers_and_the_dispatcher
```

Fails at 69668888a on the async lane's Mac and failed in the lane's run at afde7b25e.
