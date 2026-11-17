# ES256 signing uses crypto-bigint covered by a timing advisory

**Status:** Closed
**Resolution:** ES256 signing moves to p256 0.14, whose elliptic-curve 0.14 resolves crypto-bigint 0.7.11, and the Apple host names the new series for feature unification.
**Systems:** Cargo.lock, data/src/crypto/ecdsa.rs, host/apple/Cargo.toml
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `Cargo.lock:1033` pins `crypto-bigint` 0.5.5. The current
[RustCrypto advisory](https://rustsec.org/advisories/RUSTSEC-2026-0387.html)
reports secret-dependent execution in operations intended to be constant
time; its patched range begins at 0.7.6. Practical leakage depends on the
operation, compiler, CPU and an attacker's ability to observe execution.

This is a production signing dependency. `data/Cargo.toml:16` selects
`p256` 0.13, whose `elliptic-curve` dependency resolves to this
`crypto-bigint`. `data/src/crypto/ecdsa.rs:245–256` signs ES256 proofs with
that implementation. The same code serves native Rust sources and the
Hermes `crypto.subtle` adapter. `host/apple/Cargo.toml:57` also directly
selects the old `crypto-bigint` series to unify build features.

Validation: the complete Cargo lockfile's OSV scan identifies
RUSTSEC-2026-0387, and `cargo tree --offline --locked -i crypto-bigint
--depth 4` confirms the signing dependency chain. The key generation and
signing call sites were inspected. No timing attack or key recovery was
performed, and this report does not claim that every affected arithmetic
operation is exercised by P-256.

Move the signing dependency graph to a maintained combination containing
the fixes, or obtain a supported backport for the selected versions.
Changing only the lockfile to 0.7.6 cannot satisfy the current 0.5 version
requirements. Update the Apple feature-unification dependency alongside
the signing graph. Preserve existing JWK, ES256, DPoP and cross-executor
tests when evaluating the replacement.

Checked open and closed filesystem issues and all 275 cached GitHub issues
for crypto-bigint, timing leakage, ES256 and RUSTSEC-2026-0387. No matching
report was found.
