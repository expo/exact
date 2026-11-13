# The Snapback dependency pins undici with published security fixes missing

**Status:** Closed
**Resolution:** The root package.json overrides Snapback 0.4.33's undici with 8.12.2, the version Snapback 0.4.43 ships with the same Node transport, until the client checkout and its Rust revision move to that release.
**Systems:** package.json, bun.lock, Snapback development tooling
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, the root dev dependency `snapback4` 0.4.33 requires exactly
`undici` 8.11.2 (`bun.lock:532,540`). Running the pinned Bun's
`bun audit --json` reports eight advisories for that version. The patched
8.x version is 8.12.2.

The installed Snapback Node transport imports undici's `Agent`, `Client`
and `Pool` directly and passes its dispatcher to `globalThis.fetch`
(`snapback4/dist/node-fetch.js`). Its agent options do not restrict TLS to
1.3. This is relevant to
[GHSA-qrxm-fqhp-jwvm](https://github.com/nodejs/undici/security/advisories/GHSA-qrxm-fqhp-jwvm):
a malicious HTTPS endpoint can terminate a Node process through a TLS 1.2
renegotiation error that the affected dispatcher treats as an impossible
condition. Catching the fetch promise's rejection does not address an
uncaught socket-handler assertion.

Scope: this is a development dependency and its Node transport. Exact's
browser, URLSession and Rust transports are separate. The other audit
matches include optional APIs and configurations; the eight matches do
not establish eight reachable vulnerabilities in Exact. No malicious TLS
server was run during this review.

Update the Snapback pin to a compatible release using a patched undici,
or coordinate a tested dependency override if that release is not yet
available. Keep the native and TypeScript Snapback pins aligned where the
repository requires it. Re-run the audit and Snapback's transport/retry
tests: this adapter imports private undici modules, so a version change
needs a compatibility check.

Validation: `bun install --frozen-lockfile`, the installed package's
transport source, and the registry audit agree on the exact version. The
audit result is saved at `/tmp/exact-review-20261112/bun-audit.json`.
Checked open and closed filesystem issues and all 275 cached GitHub issues
for undici and the returned advisory identifiers. No matching report was
found.
