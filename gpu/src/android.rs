//! Android's initialization entry (LLP 1009 D11): what a GPU artifact that
//! calls into Java needs from the app, once, before its first surface.
//!
//! The host opens the artifact with `dlopen`, which runs no `JNI_OnLoad`, so
//! an artifact that exports `gpu_android_init` is given the process's Java
//! VM, the calling thread's JNI environment and the application `Context`
//! there instead ([`android_init!`](crate::android_init)).

use std::ffi::c_void;

/// What the host gives the entry. The pointers are JNI's own types, untyped
/// here so that an artifact chooses its JNI binding, or none.
#[derive(Clone, Copy, Debug)]
pub struct Android {
    /// The process's `JavaVM*`: attach a thread the artifact starts with it.
    pub vm: *mut c_void,
    /// The calling thread's `JNIEnv*`, valid for this call only. The thread
    /// may be one the host attached, where `FindClass` sees the system class
    /// loader alone: an app's class is loaded through
    /// `context.getClassLoader()`.
    pub env: *mut c_void,
    /// The application `Context` (never an `Activity`): a global reference
    /// the host keeps for the process, usable on any attached thread.
    pub context: *mut c_void,
}

/// `gpu_android_init`'s body: 0 when `initialize` succeeds, else 1 with its
/// error where `gpu_error` reads it on this thread.
#[cfg(not(target_arch = "wasm32"))]
pub fn init<E: std::fmt::Display>(
    android: Android,
    initialize: impl FnOnce(Android) -> Result<(), E>,
) -> u32 {
    crate::report_panics();
    match initialize(android) {
        Ok(()) => 0,
        Err(error) => {
            crate::native::refuse(&error.to_string());
            1
        }
    }
}

/// Define the artifact's Android initialization entry, in the crate that
/// calls [`module!`](crate::module): one expression that takes an
/// [`Android`] and returns a `Result` whose error is `Display`, composing the
/// initializers of the crate's dependencies. Every target has the macro, so
/// the line needs no `cfg`; off Android it compiles to nothing.
///
/// ```ignore
/// exact_gpu::android_init!(|android| unsafe {
///     mln_storage::android::init(android.env, android.context)
/// });
/// ```
///
/// The host calls it once for each loaded image in a process, on the thread
/// that opens the artifact, after it has verified the artifact's baked
/// identity and before any other export (so before `gpu_prepare`, a device or
/// a surface factory). A failure, or a Java exception left pending, refuses
/// the artifact: its canvases report the error and stay unpainted. An
/// initialized artifact stays loaded for the life of the process, as a
/// library the VM loaded does.
#[macro_export]
macro_rules! android_init {
    ($initialize:expr) => {
        #[cfg(target_os = "android")]
        $crate::android_init!(@entry $initialize);
    };
    (@entry $initialize:expr) => {
        /// Android's initialization entry (LLP 1009 D11). 0 on success.
        ///
        /// # Safety
        /// `vm`, `env` and `context` are the process's `JavaVM*`, the calling
        /// thread's `JNIEnv*` and a live reference to the application
        /// `Context`.
        #[no_mangle]
        pub unsafe extern "C" fn gpu_android_init(
            vm: *mut ::std::ffi::c_void,
            env: *mut ::std::ffi::c_void,
            context: *mut ::std::ffi::c_void,
        ) -> u32 {
            $crate::android::init($crate::android::Android { vm, env, context }, $initialize)
        }
    };
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::ffi::c_void;

    // The entry as an Android artifact exports it, on every test host.
    crate::android_init!(@entry |android| {
        if android.context.is_null() {
            return Err("no application Context");
        }
        GIVEN.set(Some([android.vm, android.env, android.context].map(|p| p as usize)));
        Ok(())
    });

    thread_local! {
        static GIVEN: std::cell::Cell<Option<[usize; 3]>> = const { std::cell::Cell::new(None) };
    }

    #[test]
    fn the_entry_gives_the_initializer_its_pointers_and_reports_its_error_once() {
        let [vm, env, context] = [1, 2, 3].map(|p: usize| p as *mut c_void);
        // SAFETY: the test's initializer reads the pointers as numbers.
        assert_eq!(unsafe { gpu_android_init(vm, env, context) }, 0);
        assert_eq!(GIVEN.get(), Some([1, 2, 3]));
        assert_eq!(crate::native::error(), "");
        // SAFETY: as above.
        assert_eq!(
            unsafe { gpu_android_init(vm, env, std::ptr::null_mut()) },
            1
        );
        assert_eq!(crate::native::error(), "no application Context");
        assert_eq!(crate::native::error(), "", "reported once");
    }
}
