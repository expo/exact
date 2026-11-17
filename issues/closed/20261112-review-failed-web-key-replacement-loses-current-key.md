# A failed web `keepKey` replacement loses access to the current key

**Status:** Closed
**Resolution:** `keepKey` on the web writes the new handle to the name only after the key service has stored the pair, and a later `keepKey`, `set` or `forget` of the name supersedes a write still in flight, on the JS target and in the wasm prelude.
**Systems:** web key persistence, host/web-js/admission.js, host/web/storage-environment.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Problem

If persisting a replacement CryptoKeyPair fails, `store.keepKey` rejects but
has already overwritten the current key's handle. The old key is still in
the key backend, yet `store.key(name)` now returns `null`. An app that catches
the failed replacement cannot continue using the previously kept key.

Reviewed at `f6be99af`. `host/web-js/admission.js:247-253` calls
`store.set(name, handle)` before loading the key service or awaiting
`service.put`. Nothing restores the old handle on rejection. The actual
store changes memory immediately and schedules persistence
(`host/web-js/rt.js:400-405`); its writes are kept even when settlement fails.
The wasm prelude has the same order in `js/src/prelude.js:394-396`.

`host/web/storage-environment.js:121-134` can reject opening IndexedDB or
committing its transaction, including a storage/quota failure. These failures
therefore leave a durable handle to a key that was never stored.

## Evidence

On Bun 1.4.2, used the actual facade, sealed grant validator and memory key
backend with real WebCrypto key pairs. Kept the first key successfully, then
injected `QuotaExceededError` at the key service's second `put`. The backing
secret map models the production store's immediate `set`:

```json
{"replacement":"QuotaExceededError","oldMappingLost":true,"keyAfterFailure":null,"oldKeyStillInBackend":true}
```

The error was injected at the persistence boundary; this is not a browser
quota exhaustion test.

## Fix and validation

Publish the new handle only after the key backend confirms its write, and
define cleanup if publishing the handle then fails. Preserve the old mapping
on a failed replacement. Account for concurrent replacements and explicit
forget operations while a write is pending.

Test failed backend open, failed transaction, successful replacement and a
concurrent forget. A rejected replacement must not silently destroy the
previous usable mapping or resurrect a key after it was forgotten.

## Duplicate check

Checked open and closed filesystem issues and all 275 cached GitHub issues.
`20261112-review-web-keepkey-bypasses-required-presence.md` concerns admitting
a forbidden write. This issue concerns atomicity of an admitted write.
The closed native key-handle lifetime issue concerns a different backend.
