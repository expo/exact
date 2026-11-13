//! The portable transports' TLS trust refuses the emSign keys Mozilla distrusted
//! (Mozilla bug 2083449; RUSTSEC-2026-0381 for `webpki-roots`, RUSTSEC-2026-0382
//! for `rustls-webpki`). Both transports verify with rustls's WebPKI verifier over
//! either the system's roots or, when it has none, `webpki_roots::TLS_SERVER_ROOTS`
//! (`rustls_http::tls_config_from`, ureq's `RootCerts::WebPki`, `websocket::tls_config`).
//!
//! The fixtures were made with OpenSSL's `x509 -force_pubkey`: a test root signs
//! a leaf carrying the distrusted public key of eMudhra's `EMOVTLSCAG3B`
//! intermediate, an intermediate carrying it, and a control leaf with a key of
//! its own. The leaf under the intermediate is signed by another key, since the
//! distrusted key's private half is not ours: the verifier must refuse the path
//! for its distrusted key before any signature is checked. The test keys were
//! discarded.

use std::sync::Arc;
use std::time::Duration;

use rustls::client::danger::ServerCertVerifier;
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::RootCertStore;

const ROOT: &[u8] = include_bytes!("testdata/distrusted/root.der");
const TRUSTED_LEAF: &[u8] = include_bytes!("testdata/distrusted/trusted-leaf.der");
const DISTRUSTED_LEAF: &[u8] = include_bytes!("testdata/distrusted/distrusted-leaf.der");
const DISTRUSTED_INTERMEDIATE: &[u8] =
    include_bytes!("testdata/distrusted/distrusted-intermediate.der");
const INTERMEDIATE_LEAF: &[u8] = include_bytes!("testdata/distrusted/intermediate-leaf.der");
/// A day after the fixtures were made; each is valid for a hundred years from then.
const NOW: u64 = 1_794_547_263 + 86_400;

/// The emSign roots `webpki-roots` 1.0.9 held, which it no longer takes from
/// Mozilla's store.
const DISTRUSTED_ROOTS: [&str; 3] = [
    "emSign ECC Root CA - G3",
    "emSign Root CA - C1",
    "emSign ECC Root CA - C3",
];

/// A system store holding only the test root, and the compiled-in fallback with
/// the test root added: either would admit a chain to the root.
fn stores() -> [(&'static str, RootCertStore); 2] {
    let mut system = RootCertStore::empty();
    system.add(CertificateDer::from(ROOT)).unwrap();
    let mut compiled = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    compiled.add(CertificateDer::from(ROOT)).unwrap();
    [("system", system), ("compiled-in", compiled)]
}

fn verify(
    store: &RootCertStore,
    leaf: &[u8],
    intermediates: &[&[u8]],
    name: &str,
) -> Result<(), rustls::Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = WebPkiServerVerifier::builder_with_provider(Arc::new(store.clone()), provider)
        .build()
        .unwrap();
    let intermediates: Vec<_> = intermediates
        .iter()
        .map(|der| CertificateDer::from(*der))
        .collect();
    verifier
        .verify_server_cert(
            &CertificateDer::from(leaf),
            &intermediates,
            &ServerName::try_from(name.to_owned()).unwrap(),
            &[],
            UnixTime::since_unix_epoch(Duration::from_secs(NOW)),
        )
        .map(|_| ())
}

#[test]
fn the_compiled_in_roots_hold_no_distrusted_emsign_root() {
    for anchor in webpki_roots::TLS_SERVER_ROOTS {
        let subject = String::from_utf8_lossy(anchor.subject.as_ref());
        for name in DISTRUSTED_ROOTS {
            assert!(!subject.contains(name), "the compiled-in roots hold {name}");
        }
    }
}

#[test]
fn a_path_through_a_distrusted_emsign_key_is_refused_by_either_store() {
    for (store_name, store) in stores() {
        verify(&store, TRUSTED_LEAF, &[], "trusted.test").unwrap_or_else(|error| {
            panic!("the {store_name} store refused the control: {error:?}")
        });
        for (what, leaf, intermediates, name) in [
            ("a leaf", DISTRUSTED_LEAF, &[][..], "distrusted.test"),
            (
                "an intermediate",
                INTERMEDIATE_LEAF,
                &[DISTRUSTED_INTERMEDIATE][..],
                "intermediate.test",
            ),
        ] {
            let error = verify(&store, leaf, intermediates, name).expect_err(&format!(
                "the {store_name} store admitted {what} with the key"
            ));
            assert!(
                format!("{error:?}").contains("CertDistrusted"),
                "the {store_name} store refused {what} with the key for another reason: {error:?}"
            );
        }
    }
}
