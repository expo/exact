//! The app's Kotlin data module (LLP 1067.000 D3, D9). The reader's
//! class loader resolves the bridge. Each Canvas session owns its instance.
#![allow(unsafe_code)]
use crate::presenter::Presenter;
use crate::surfaces::attach_application;
use exact_runner::{DataSource, NativeCall};
use jni_sys::{jclass, jmethodID, jobject, jvalue, JNIEnv};
use std::sync::{Arc, OnceLock};

struct Methods {
    class: jclass,
    open: jmethodID,
    call: jmethodID,
    app: jmethodID,
    timer: jmethodID,
    end: jmethodID,
}
// SAFETY: the class reference is global and its method IDs live as long as the class.
unsafe impl Sync for Methods {}
unsafe impl Send for Methods {}
static METHODS: OnceLock<Option<Methods>> = OnceLock::new();

/// Resolve through the APK's loader, before a boot moves to a Rust thread.
/// # Safety
/// `env` belongs to this JNI entry's thread.
pub unsafe fn attach(env: *mut JNIEnv) {
    METHODS.get_or_init(|| {
        let class =
            ((**env).FindClass.expect("jni"))(env, c"dev/exact/canvas/ModuleSession".as_ptr());
        let lookup = || {
            if class.is_null() {
                return None;
            }
            let method = |name: &std::ffi::CStr, sig: &std::ffi::CStr| {
                let id = ((**env).GetMethodID?)(env, class, name.as_ptr(), sig.as_ptr());
                (!id.is_null()).then_some(id)
            };
            let open = ((**env).GetStaticMethodID?)(
                env,
                class,
                c"open".as_ptr(),
                c"(Landroid/content/Context;)Ldev/exact/canvas/ModuleSession;".as_ptr(),
            );
            if open.is_null() {
                return None;
            }
            let methods = Methods {
                open,
                call: method(c"call", c"(Ljava/lang/String;)Ljava/lang/String;")?,
                app: method(c"app", c"(Ljava/lang/String;Z)V")?,
                timer: method(c"timer", c"(I)V")?,
                end: method(c"end", c"()V")?,
                class: ((**env).NewGlobalRef?)(env, class),
            };
            (!methods.class.is_null()).then_some(methods)
        };
        let methods = lookup();
        if methods.is_none() {
            ((**env).ExceptionClear.expect("jni"))(env);
            crate::android::log("exact: the reader has no app data module bridge");
        }
        ((**env).DeleteLocalRef.expect("jni"))(env, class);
        methods
    });
}

struct Instance {
    object: jobject,
    methods: &'static Methods,
}
// SAFETY: only the global reference crosses threads. Each call attaches its own thread.
unsafe impl Send for Instance {}
unsafe impl Sync for Instance {}

impl Instance {
    fn open() -> Option<Self> {
        let methods = METHODS.get()?.as_ref()?;
        let thread = attach_application().ok()?;
        let env = thread.env.cast::<JNIEnv>();
        unsafe {
            let local = ((**env).CallStaticObjectMethodA.expect("jni"))(
                env,
                methods.class,
                methods.open,
                [jvalue {
                    l: thread.context.cast(),
                }]
                .as_ptr(),
            );
            if thread.threw() || local.is_null() {
                return None;
            }
            let object = ((**env).NewGlobalRef.expect("jni"))(env, local);
            let failed = thread.threw() || object.is_null();
            if failed {
                ((**env).CallVoidMethodA.expect("jni"))(env, local, methods.end, std::ptr::null());
                thread.threw();
            }
            ((**env).DeleteLocalRef.expect("jni"))(env, local);
            (!failed).then(|| Self { object, methods })
        }
    }

    fn call(&self, body: &[u8]) -> Result<Vec<u8>, String> {
        let thread = attach_application()?;
        let env = thread.env.cast::<JNIEnv>();
        let text = std::str::from_utf8(body).map_err(|e| e.to_string())?;
        unsafe {
            let arg = crate::canvas::field::java(env, text);
            if thread.threw() {
                return Err("cannot allocate module request".into());
            }
            let value = ((**env).CallObjectMethodA.expect("jni"))(
                env,
                self.object,
                self.methods.call,
                [jvalue { l: arg }].as_ptr(),
            );
            ((**env).DeleteLocalRef.expect("jni"))(env, arg);
            if thread.threw() || value.is_null() {
                return Err("the Kotlin module call failed".into());
            }
            let answer = crate::canvas::field::text(env, value);
            ((**env).DeleteLocalRef.expect("jni"))(env, value);
            match answer.strip_prefix('=') {
                Some(json) => Ok(json.as_bytes().to_vec()),
                None => Err(answer.strip_prefix('!').unwrap_or(&answer).to_owned()),
            }
        }
    }

    fn notify(&self, method: jmethodID, args: &[jvalue]) {
        if let Ok(thread) = attach_application() {
            let env = thread.env.cast::<JNIEnv>();
            unsafe {
                ((**env).CallVoidMethodA.expect("jni"))(env, self.object, method, args.as_ptr());
            }
            thread.threw();
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.notify(self.methods.end, &[]);
        if let Ok(thread) = attach_application() {
            let env = thread.env.cast::<JNIEnv>();
            unsafe { ((**env).DeleteGlobalRef.expect("jni"))(env, self.object) };
        }
    }
}

#[derive(Default)]
pub(crate) struct Session {
    looked: bool,
    instance: Option<Arc<Instance>>,
    pub(crate) timers: crate::hatches::session::Afters<i32>,
}

impl Session {
    pub(crate) fn call(&self) -> Option<NativeCall> {
        let module = Arc::downgrade(self.instance.as_ref()?);
        Some(Arc::new(move |request| {
            module
                .upgrade()
                .ok_or("the module's session ended")?
                .call(request)
        }))
    }

    pub(crate) fn app(&self, app: &crate::hatches::App) {
        let Some(module) = &self.instance else { return };
        let Ok(thread) = attach_application() else {
            return;
        };
        let env = thread.env.cast::<JNIEnv>();
        unsafe {
            let words = crate::canvas::field::java(
                env,
                &serde_json::to_string(&app.data).expect("app words"),
            );
            if thread.threw() {
                return;
            }
            module.notify(
                module.methods.app,
                &[
                    jvalue { l: words },
                    jvalue {
                        z: app.is_new.into(),
                    },
                ],
            );
            ((**env).DeleteLocalRef.expect("jni"))(env, words);
        }
    }

    pub(crate) fn fire(&self, timer: i32, now: f64) {
        crate::canvas::clock::advance(now);
        if let Some(module) = &self.instance {
            module.notify(module.methods.timer, &[jvalue { i: timer }]);
        }
    }
}

impl<D: DataSource> Presenter<D> {
    pub(crate) fn open_android_module(&mut self) {
        let session = &mut self.requests.app_module;
        if std::mem::replace(&mut session.looked, true) {
            return;
        }
        let Some(module) = Instance::open() else {
            return;
        };
        session.instance = Some(Arc::new(module));
        self.hatches
            .install
            .get_or_insert_with(|| crate::hatches::install_with_reader::<()>(&[]));
        self.install_shell_handlers();
    }

    pub(crate) fn module_after(&mut self, timer: i32, due: f64) {
        self.requests.app_module.timers.add(due, timer);
    }
}
