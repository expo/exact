# Portable TLS still trusts the affected emSign certificate chain

**Status:** Open
**Systems:** Cargo.lock, vendored Ibex HTTP and WebSocket transports
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P1

At `f6be99af`, `Cargo.lock` resolves `webpki-roots` to 1.0.9 (line 6777)
and `rustls-webpki` to 0.103.15 (line 5105). Both are affected by
CVE-2026-111943: certificate paths through distrusted emSign intermediate
keys can still be accepted when the trust store permits them. The
corresponding private keys are public. The fixes remove the affected root
in `webpki-roots` 1.0.10 and reject the intermediate keys in
`rustls-webpki` 0.103.16. See the
[root-store advisory](https://rustsec.org/advisories/RUSTSEC-2026-0381.html)
and [verifier advisory](https://rustsec.org/advisories/RUSTSEC-2026-0382.html).

This dependency is used by the portable transports. In
`vendor/ibex/crates/ibex2/src/transport/rustls_http.rs:191–217`, an empty
system trust store selects ureq's compiled WebPKI roots. In
`transport/websocket.rs:227–242`, the same condition directly loads
`webpki_roots::TLS_SERVER_ROOTS`. Both use the affected verifier. An
otherwise valid forged chain can therefore authenticate an attacker as a
granted HTTPS or WSS origin on this path. Origin grants do not replace TLS
server authentication.

The exposure depends on the configured trust anchors. The compiled-root
fallback is affected; a system store that already distrusts the chain can
prevent that path. Native Apple URLSession and browser TLS are separate
implementations and are not implicated by this finding.

Validation: parsed the complete lockfile, queried its 438 registry
dependencies against OSV, checked the two primary advisories, and traced
both transport configurations. `cargo tree --offline --locked --workspace
--target all -i rustls -e features --depth 4` confirms the production
portable transport graph. No forged certificate or live interception was
used; this is a dependency and configuration finding.

Update both resolved dependencies to patched versions and check the
portable HTTP and WebSocket TLS tests. Include a regression that refuses
the distrusted intermediate with a trust store that would otherwise admit
its chain, covering the compiled-root fallback too.

Checked open and closed filesystem issues and all 275 cached GitHub issues
for emSign, WebPKI, RustSec and the advisory identifiers. No matching report
was found.
