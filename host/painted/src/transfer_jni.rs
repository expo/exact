//! The package's one JNI entry. Expanded only for an importing app.
#![allow(unsafe_code)]
use jni_sys::{jbyteArray, JNIEnv};

/// Call from the package's worker, including in a process with no session.
/// # Safety
/// `env` and `request` belong to this JNI call.
pub unsafe fn call(env: *mut JNIEnv, request: jbyteArray, compat: &str) -> jbyteArray {
    crate::canvas::secrets::attach(env);
    let text = crate::canvas::jni::utf8(env, request);
    let result = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| {
            let compat: serde_json::Value = serde_json::from_str(compat).ok()?;
            Some(super::dispatch(
                &v,
                compat["inputs"]["grantCeiling"].as_str().unwrap_or(""),
            ))
        })
        .unwrap_or(Err("TypeError"));
    let result = result.unwrap_or_else(|code| serde_json::json!({"error": code}));
    crate::canvas::jni::bytes(env, result.to_string().as_bytes())
}

/// Emit only the imported package's entry point, without a core Cargo feature.
#[macro_export]
macro_rules! android_transfer_jni {
    () => {
        #[allow(unsafe_code, clippy::missing_safety_doc)]
        #[no_mangle]
        pub unsafe extern "system" fn Java_exact_backgroundtransfer_NativeTransfer_call(
            env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            request: $crate::canvas::jni_sys::jbyteArray,
        ) -> $crate::canvas::jni_sys::jbyteArray {
            $crate::transfer::call(
                env,
                request,
                include_str!(concat!(env!("OUT_DIR"), "/compat.json")),
            )
        }
    };
}
