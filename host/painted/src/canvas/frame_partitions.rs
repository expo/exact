//! `exactPage().iframePartition` on the Android reader (LLP 1020.002 D1):
//! whether the device's WebView keeps a partition, which only the reader can
//! ask (`dev.exact.canvas.Partitions.keeps`). Asking loads the WebView
//! provider, so only a plan with an `iframe` asks, once, at boot.
#![allow(unsafe_code)]
use jni_sys::{jclass, jmethodID, JNIEnv, JavaVM, JNI_OK, JNI_VERSION_1_6};
use std::{ffi::c_void, sync::OnceLock};

struct Reader {
    vm: *mut JavaVM,
    class: jclass,
    method: jmethodID,
}
// The VM, global class reference and method ID are valid across threads.
unsafe impl Send for Reader {}
unsafe impl Sync for Reader {}
static READER: OnceLock<Reader> = OnceLock::new();

/// Find the reader's method from a JNI entry, whose class loader sees it.
/// # Safety
/// `env` belongs to this JNI entry and thread.
pub unsafe fn attach(env: *mut JNIEnv) {
    if READER.get().is_some() {
        return;
    }
    let read = || {
        let mut vm = std::ptr::null_mut();
        if ((**env).GetJavaVM?)(env, &mut vm) != JNI_OK {
            return None;
        }
        let local = ((**env).FindClass?)(env, c"dev/exact/canvas/Partitions".as_ptr());
        if local.is_null() {
            ((**env).ExceptionClear?)(env);
            return None;
        }
        let class = ((**env).NewGlobalRef?)(env, local);
        ((**env).DeleteLocalRef?)(env, local);
        if class.is_null() {
            ((**env).ExceptionClear?)(env);
            return None;
        }
        let method = ((**env).GetStaticMethodID?)(env, class, c"keeps".as_ptr(), c"()Z".as_ptr());
        if method.is_null() {
            ((**env).ExceptionClear?)(env);
            ((**env).DeleteGlobalRef?)(env, class);
            return None;
        }
        Some(Reader { vm, class, method })
    };
    if let Some(reader) = read() {
        let _ = READER.set(reader);
    }
}

/// Whether the reader's WebView keeps partitions; false if it cannot be asked.
pub fn kept() -> bool {
    let Some(reader) = READER.get() else {
        return false;
    };
    // SAFETY: attach retained the class globally; a thread attached here is
    // detached before returning.
    unsafe {
        let vm = reader.vm;
        let mut env: *mut JNIEnv = std::ptr::null_mut();
        let out = (&mut env as *mut *mut JNIEnv).cast::<*mut c_void>();
        let Some(get) = (**vm).GetEnv else {
            return false;
        };
        let attached = get(vm, out, JNI_VERSION_1_6) != JNI_OK;
        if attached {
            let Some(attach) = (**vm).AttachCurrentThread else {
                return false;
            };
            if attach(vm, out, std::ptr::null_mut()) != JNI_OK {
                return false;
            }
        }
        let kept = match (
            (**env).CallStaticBooleanMethodA,
            (**env).ExceptionCheck,
            (**env).ExceptionClear,
        ) {
            (Some(call), Some(check), Some(clear)) => {
                let kept = call(env, reader.class, reader.method, std::ptr::null()) != 0;
                if check(env) != 0 {
                    clear(env);
                    false
                } else {
                    kept
                }
            }
            _ => false,
        };
        if attached {
            if let Some(detach) = (**vm).DetachCurrentThread {
                detach(vm);
            }
        }
        kept
    }
}
