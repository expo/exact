# Web `keepKey` stores a key whose grant requires user presence

**Status:** Open
**Systems:** web secret storage, host/web-js/admission.js, host/web/storage-environment.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P1

## Problem

`store.keepKey` accepts a name with mandatory user-presence protection and
stores its private key on the web, where that protection is unavailable.
It reports success even though subsequent `store.key` calls return `null`.

Reviewed at `f6be99af`. `createSecretFacade` rejects ordinary `set` and
`exclusive` calls for a name whose `secret.keep` line requires presence
(`host/web-js/admission.js:209-245`). Its `keepKey` method at lines 247-253
checks the grant but omits the same `mustLock` check. It directly writes a
handle and calls the key service's `put`. `keyStore` in
`host/web/storage-environment.js:116-135` stores the CryptoKeyPair in
IndexedDB without a presence gate.

The private key can remain non-extractable; that does not provide user
presence or prevent code with access to the stored CryptoKey from using it
to sign. The explicit contract in `docs/contract-for-agents.md:1284-1285`
is that the web refuses a name that must be locked.

## Evidence

On Bun 1.4.2, imported the actual facade and grant validator with a correctly
sealed `secret.keep signing user-presence` declaration. Generated a real
non-extractable ECDSA P-256 key pair with WebCrypto. A recording store and
key-service substitute produced:

```json
{"plainSet":"NotSupportedError","keepKey":"fulfilled","handleKept":true,"privateKeyKept":true,"keyRead":null}
```

The key service received the original private CryptoKey. This is a
module-level admission test; the IndexedDB backend was not exercised.

## Fix and validation

Apply mandatory presence admission before writing either the handle or the
key. Reject with the same `NotSupportedError` as ordinary secret writes.
Test every write API against mandatory and optional presence policies, and
assert that refusal leaves both storage backends unchanged.

## Duplicate check

Checked open and closed filesystem issues and all 275 cached GitHub issues.
The closed `20261102-native-crypto-key-handles-never-released.md` concerns
native in-memory handle lifetime. The closed presence-protect and reinstall
keychain issues concern Apple vault cleanup. None covers this web admission
bypass.
