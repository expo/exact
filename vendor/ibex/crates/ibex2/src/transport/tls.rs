//! Trust policy shared by HTTP and WebSocket TLS.
#[cfg(test)]
use std::sync::atomic::Ordering;
#[cfg(feature = "websocket")]
use std::sync::Arc;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Roots {
    System(usize),
    CompiledIn,
    PlatformVerifier,
}
impl std::fmt::Display for Roots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System(n) => write!(f, "the system's trust store ({n} roots)"),
            Self::PlatformVerifier => write!(f, "the platform certificate verifier"),
            Self::CompiledIn => write!(f, "no system trust store; the compiled-in roots"),
        }
    }
}

// Every transport in the process shares the one trust-store read. Keeping the
// config out of `new` also keeps constructing an unused host endowment cheap.
// @ref LLP 0068#5-open-questions — native roots are loaded lazily and once per process
pub(super) fn tls_config() -> &'static (ureq::tls::TlsConfig, Roots) {
    static TLS_CONFIG: OnceLock<(ureq::tls::TlsConfig, Roots)> = OnceLock::new();
    TLS_CONFIG.get_or_init(|| {
        tls_config_from(cfg!(target_os = "android"), || {
            #[cfg(test)]
            NATIVE_CERT_LOADS.fetch_add(1, Ordering::SeqCst);
            rustls_native_certs::load_native_certs().unwrap_or_default()
        })
    })
}

#[cfg(test)]
pub(super) static NATIVE_CERT_LOADS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub(super) fn tls_config_from(
    android: bool,
    load_certs: impl FnOnce() -> Vec<rustls::pki_types::CertificateDer<'static>>,
) -> (ureq::tls::TlsConfig, Roots) {
    if android {
        return (
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
            Roots::PlatformVerifier,
        );
    }
    let certs = load_certs();
    let mut store = rustls::RootCertStore::empty();
    let mut certificates = Vec::new();
    for cert in certs {
        if store.add(cert.clone()).is_ok() {
            certificates.push(ureq::tls::Certificate::from_der(cert.as_ref()).to_owned());
        }
    }
    let (root_certs, roots) = if certificates.is_empty() {
        (ureq::tls::RootCerts::WebPki, Roots::CompiledIn)
    } else {
        let count = certificates.len();
        (certificates.into(), Roots::System(count))
    };
    (
        ureq::tls::TlsConfig::builder()
            .root_certs(root_certs)
            .build(),
        roots,
    )
}
#[cfg(feature = "websocket")]
pub(super) fn socket_config() -> Arc<rustls::ClientConfig> {
    let builder = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("ring supports the default protocol versions");
    let builder = match tls_config().0.root_certs() {
        #[cfg(target_os = "android")]
        ureq::tls::RootCerts::PlatformVerifier => {
            use rustls_platform_verifier::BuilderVerifierExt;
            builder
                .with_platform_verifier()
                .expect("the Android verifier")
        }
        ureq::tls::RootCerts::Specific(certs) => {
            let mut store = rustls::RootCertStore::empty();
            for cert in certs.iter() {
                store
                    .add(rustls::pki_types::CertificateDer::from(cert.der().to_vec()))
                    .expect("the shared trust store already validated this certificate");
            }
            builder.with_root_certificates(store)
        }
        ureq::tls::RootCerts::WebPki => builder.with_root_certificates(rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        }),
        _ => unreachable!("the shared trust policy supplies every root configuration"),
    };
    Arc::new(builder.with_no_client_auth())
}

pub(super) fn ensure_available() -> Result<(), crate::boundary::HostError> {
    #[cfg(target_os = "android")]
    if !ANDROID_READY.load(std::sync::atomic::Ordering::Acquire) {
        return Err(crate::boundary::HostError::Failed(
            "Android TLS requires initialization with the application's JVM context".into(),
        ));
    }
    Ok(())
}

#[cfg(target_os = "android")]
static ANDROID_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Initialize Android's certificate verifier before starting network work.
///
/// # Safety
/// `env` and `context` must be valid JNI pointers for the calling thread and its local frame.
#[cfg(target_os = "android")]
pub unsafe fn initialize_android(env: *mut std::ffi::c_void, context: *mut std::ffi::c_void) {
    let mut env = unsafe { jni::EnvUnowned::from_raw(env.cast()) };
    env.with_env(|env| -> jni::errors::Result<()> {
        let context = unsafe { jni::objects::JObject::from_raw(env, context.cast()) };
        rustls_platform_verifier::android::init_with_env(env, context)?;
        ANDROID_READY.store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}
