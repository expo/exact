//! A GPU artifact's Android initialization entry (LLP 1009 D11): `dlopen`
//! runs no `JNI_OnLoad`, so an artifact that calls into Java exports
//! `gpu_android_init` and the host calls it with the Java VM, the calling
//! thread's JNI environment and the application `Context`, on whichever
//! thread opens the artifact, before any other export. An artifact without
//! the export costs one failed symbol lookup.
#![allow(unsafe_code)]
use super::Abi;
use libloading::Library;
use std::{collections::BTreeMap, ffi::c_void, path::Path, sync::Mutex};

/// `gpu_android_init(JavaVM*, JNIEnv*, jobject)` (`gpu/src/android.rs`): 0,
/// or a failure `gpu_error` describes on the same thread.
type Entry = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> u32;

/// What each image's entry answered, by the entry's address, which names one
/// image for the process because an image whose entry ran is never unloaded.
/// Held across the call: a second opener of the image waits for the answer.
static ANSWERS: Mutex<BTreeMap<usize, Result<(), String>>> = Mutex::new(BTreeMap::new());

const UNBOUND: &str = "the reader gave the host no application Context";

/// Run the entry of `abi`'s image, opened from `path`, unless it has run in
/// this process; its failure then and now refuses the artifact.
pub(super) fn run(abi: &Abi, path: &Path) -> Result<(), String> {
    // SAFETY: the export's declared signature; only its address is read here.
    let Ok(entry) = (unsafe { abi.library.get::<Entry>(b"gpu_android_init") }).map(|e| *e) else {
        return Ok(());
    };
    let refuse = |why: &str| {
        format!(
            "GPU module {}: Android initialization failed: {why}",
            path.display()
        )
    };
    let mut answers = ANSWERS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(answer) = answers.get(&(entry as usize)) {
        return answer.clone();
    }
    // An entry that could not be called has not run: a later open calls it.
    let jvm = jvm::attach().map_err(|e| refuse(&e))?;
    // The image keeps what the entry gave it (global references, native
    // methods a class was registered with), so it stays loaded for the
    // process, as a library the VM loaded does: one more reference, kept.
    // SAFETY: the module `abi` holds open, verified before it was opened.
    std::mem::forget(unsafe { Library::new(path) }.map_err(|e| refuse(&e.to_string()))?);
    // SAFETY: the entry's contract: the VM, this thread's environment and a
    // global reference to the application.
    let code = unsafe { entry(jvm.vm, jvm.env, jvm.context) };
    let answer = if jvm.threw() {
        Err(refuse("it left a Java exception pending"))
    } else if code != 0 {
        let why = abi.error().unwrap_or("it gave no reason".into());
        Err(refuse(&why))
    } else {
        Ok(())
    };
    answers.insert(entry as usize, answer.clone());
    answer
}

#[cfg(target_os = "android")]
pub(crate) use jvm::bind;

mod jvm {
    use jni_sys::{jobject, JNIEnv, JavaVM, JNI_OK, JNI_VERSION_1_6};
    use std::{ffi::c_void, sync::OnceLock};

    struct Application {
        vm: *mut JavaVM,
        context: jobject,
    }
    // SAFETY: a JavaVM pointer and a global reference are valid on every thread.
    unsafe impl Send for Application {}
    unsafe impl Sync for Application {}
    static APPLICATION: OnceLock<Application> = OnceLock::new();

    /// Keep the VM and a global reference to `context`, the application's
    /// `Context`, for the process: the reader's `trust`, before any host boots.
    ///
    /// `false` when a Java exception is pending (`NewGlobalRef` ran out of
    /// memory). The caller then returns to Java, which throws it, without
    /// another JNI call: ART aborts on most calls made with one pending
    /// ([JNI tips](https://developer.android.com/ndk/guides/jni-tips#exceptions)).
    /// # Safety
    /// `env` and `context` belong to the calling JNI entry and thread, with
    /// no exception pending.
    pub(crate) unsafe fn bind(env: *mut JNIEnv, context: jobject) -> bool {
        if APPLICATION.get().is_some() || context.is_null() {
            return true;
        }
        let keep = || {
            let mut vm = std::ptr::null_mut();
            if ((**env).GetJavaVM?)(env, &mut vm) != JNI_OK {
                return None;
            }
            let context = ((**env).NewGlobalRef?)(env, context);
            (!context.is_null()).then_some(Application { vm, context })
        };
        let Some(application) = keep() else {
            return (**env).ExceptionCheck.is_some_and(|check| check(env) == 0);
        };
        if let Err(unused) = APPLICATION.set(application) {
            if let Some(delete) = (**env).DeleteGlobalRef {
                delete(env, unused.context);
            }
        }
        true
    }

    /// The calling thread in the VM for one call of an entry, detached again
    /// when the host attached it.
    pub(super) struct Attached {
        pub vm: *mut c_void,
        pub env: *mut c_void,
        pub context: *mut c_void,
        detach: bool,
    }

    pub(super) fn attach() -> Result<Attached, String> {
        let application = APPLICATION.get().ok_or(super::UNBOUND)?;
        let vm = application.vm;
        let mut env: *mut JNIEnv = std::ptr::null_mut();
        let out = (&mut env as *mut *mut JNIEnv).cast::<*mut c_void>();
        // SAFETY: the process's VM, which outlives every thread of it.
        let detach = unsafe {
            let detach = ((**vm).GetEnv.ok_or("jni")?)(vm, out, JNI_VERSION_1_6) != JNI_OK;
            if detach
                && ((**vm).AttachCurrentThread.ok_or("jni")?)(vm, out, std::ptr::null_mut())
                    != JNI_OK
            {
                return Err("its thread could not attach to the Java VM".into());
            }
            detach
        };
        Ok(Attached {
            vm: vm.cast(),
            env: env.cast(),
            context: application.context.cast(),
            detach,
        })
    }

    impl Attached {
        /// Whether a Java exception is pending: logged and cleared, so that it
        /// neither surfaces in the reader's Kotlin nor aborts the detach.
        pub(super) fn threw(&self) -> bool {
            let env = self.env.cast::<JNIEnv>();
            // SAFETY: this thread's environment, attached until `self` drops.
            unsafe {
                let threw = (**env).ExceptionCheck.is_some_and(|check| check(env) != 0);
                if threw {
                    if let Some(describe) = (**env).ExceptionDescribe {
                        describe(env);
                    }
                    if let Some(clear) = (**env).ExceptionClear {
                        clear(env);
                    }
                }
                threw
            }
        }
    }

    impl Drop for Attached {
        fn drop(&mut self) {
            if self.detach {
                let vm = self.vm.cast::<JavaVM>();
                // SAFETY: the VM this thread was attached to in `attach`.
                unsafe {
                    if let Some(detach) = (**vm).DetachCurrentThread {
                        detach(vm);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "android_init_tests.rs"]
mod tests;
