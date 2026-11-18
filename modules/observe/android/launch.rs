// The same queue, dispatch and panic recovery as Linux, in Android's app cache.
include!("../service.rs");

const PRESENT_METHOD: &str = "frame-commit";

fn platform_meta() -> Map<String, Value> {
    object(json!({
        "osType": "android",
        "osName": "Android",
        "osVersion": property(c"ro.build.version.release"),
        "deviceModel": property(c"ro.product.model"),
        "deviceName": property(c"ro.product.manufacturer"),
        "clientVersion": CLIENT_VERSION,
    }))
}

// The reader does not expose battery or connectivity to launch modules.
fn device() -> Map<String, Value> {
    Map::new()
}

/// Android's public bionic property API; missing properties are omitted from the wire.
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
fn property(name: &std::ffi::CStr) -> Option<String> {
    unsafe extern "C" {
        fn __system_property_get(
            name: *const std::ffi::c_char,
            value: *mut std::ffi::c_char,
        ) -> i32;
    }
    // PROP_VALUE_MAX in <sys/system_properties.h>.
    let mut value = [0u8; 92];
    // SAFETY: a NUL-terminated name and the API's maximum writable value buffer.
    let n = unsafe { __system_property_get(name.as_ptr(), value.as_mut_ptr().cast()) };
    (n > 0).then(|| String::from_utf8_lossy(&value[..n as usize]).into_owned())
}

fn build_identity() -> Option<String> {
    let apk = PathBuf::from(std::env::var_os("EXACT_ANDROID_APK")?);
    // `/proc/self/exe` is Android's system app_process, shared across APK updates.
    journal::file_digest(&apk)
}
