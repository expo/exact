//! Remote pictures through the platform's `HttpEngine` (API 34 and later),
//! the reader's `ImageFetch` (Kotlin): every picture of an origin over one
//! HTTP/2 connection, as the platform's own clients have it. Below API 34,
//! in a reader without the class, or where the engine cannot be built,
//! `image/remote.rs` keeps ibex2's rustls transport (HTTP/1.1, a connection
//! per download).
#![allow(unsafe_code)]
use ibex2::boundary::HostError;
use ibex2::stdlib::abort::{AbortRegistration, AbortSignal};
use ibex2::stdlib::fetch::{Body, BodySource, Headers, Request, StreamingResponse, Transport};
use jni_sys::{
    jclass, jfieldID, jmethodID, jobject, jvalue, JNIEnv, JavaVM, JNI_OK, JNI_VERSION_1_6,
};
use std::ffi::{c_void, CStr};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

const CHUNK: i32 = 64 * 1024;

pub struct Engine {
    vm: *mut JavaVM,
    class: jclass,
    start: jmethodID,
    head: jmethodID,
    read: jmethodID,
    cancel: jmethodID,
    available: jmethodID,
    status: jfieldID,
    headers: jfieldID,
    failure: jfieldID,
    /// Whether the engine could be built, asked at the first picture.
    ready: OnceLock<bool>,
}
// SAFETY: a JavaVM pointer, a global class reference and method and field
// ids are the process's, valid on every thread.
unsafe impl Send for Engine {}
unsafe impl Sync for Engine {}

static ENGINE: OnceLock<Option<Engine>> = OnceLock::new();

/// The platform's transport, once [`attach`] found it and its engine could
/// be built; otherwise the rustls client for the process.
pub fn transport() -> Option<&'static Engine> {
    let engine = ENGINE.get()?.as_ref()?;
    let ready = engine.ready.get_or_init(|| {
        engine
            .with(|env| unsafe {
                let call = (**env).CallStaticBooleanMethodA.expect("jni");
                Ok(call(env, engine.class, engine.available, [].as_ptr()) != 0)
            })
            .unwrap_or(false)
    });
    ready.then_some(engine)
}

/// Find `ImageFetch` from a JNI entry, whose class loader sees the reader's
/// classes (a download thread's would not), and ask it to install.
///
/// # Safety
/// `env` and `context` are the JNI call's.
pub unsafe fn attach(env: *mut JNIEnv, context: jobject) {
    // An exception the caller left pending (the TLS verifier's) is Java's to
    // throw, and no JNI call may run under it.
    if ((**env).ExceptionCheck.expect("jni"))(env) != 0 {
        return;
    }
    ENGINE.get_or_init(|| {
        let engine = find(env, context);
        if ((**env).ExceptionCheck.expect("jni"))(env) != 0 {
            ((**env).ExceptionClear.expect("jni"))(env);
        }
        engine
    });
}

unsafe fn find(env: *mut JNIEnv, context: jobject) -> Option<Engine> {
    let mut vm = std::ptr::null_mut();
    if ((**env).GetJavaVM?)(env, &mut vm) != JNI_OK {
        return None;
    }
    let local = ((**env).FindClass?)(env, c"dev/exact/canvas/ImageFetch".as_ptr());
    if local.is_null() {
        return None;
    }
    // Each lookup returns at the first missing id: a failed one leaves
    // `NoSuchMethodError` pending, and no JNI call may run under it.
    let static_method = |name: &CStr, signature: &CStr| {
        let id = ((**env).GetStaticMethodID?)(env, local, name.as_ptr(), signature.as_ptr());
        (!id.is_null()).then_some(id)
    };
    let method = |name: &CStr, signature: &CStr| {
        let id = ((**env).GetMethodID?)(env, local, name.as_ptr(), signature.as_ptr());
        (!id.is_null()).then_some(id)
    };
    let field = |name: &CStr, signature: &CStr| {
        let id = ((**env).GetFieldID?)(env, local, name.as_ptr(), signature.as_ptr());
        (!id.is_null()).then_some(id)
    };
    let install = static_method(c"install", c"(Landroid/content/Context;)Z")?;
    let engine = Engine {
        vm,
        class: std::ptr::null_mut(),
        start: static_method(
            c"start",
            c"(Ljava/lang/String;Ljava/lang/String;)Ldev/exact/canvas/ImageFetch;",
        )?,
        head: method(c"head", c"(J)Z")?,
        read: method(c"read", c"([BIJ)I")?,
        cancel: method(c"cancel", c"()V")?,
        available: static_method(c"available", c"()Z")?,
        status: field(c"status", c"I")?,
        headers: field(c"headers", c"Ljava/lang/String;")?,
        failure: field(c"failure", c"Ljava/lang/String;")?,
        ready: OnceLock::new(),
    };
    if ((**env).CallStaticBooleanMethodA?)(env, local, install, [jvalue { l: context }].as_ptr())
        == 0
    {
        return None;
    }
    let class = ((**env).NewGlobalRef?)(env, local);
    (!class.is_null()).then_some(Engine { class, ..engine })
}

/// A thread attached for a call stays attached until it ends, as download
/// threads never do.
struct Attached(*mut JavaVM);
impl Drop for Attached {
    fn drop(&mut self) {
        // SAFETY: this thread attached itself to this VM.
        unsafe { ((**self.0).DetachCurrentThread.expect("jni"))(self.0) };
    }
}
thread_local!(static ATTACHED: std::cell::OnceCell<Attached> = const { std::cell::OnceCell::new() });

impl Engine {
    /// Run `f` with this thread's environment; a Java exception is cleared
    /// and fails the call.
    fn with<T>(&self, f: impl FnOnce(*mut JNIEnv) -> Result<T, String>) -> Result<T, HostError> {
        let env = self.env()?;
        // SAFETY: an environment is used only on the thread it belongs to,
        // and local references die with the frame.
        unsafe {
            if ((**env).PushLocalFrame.expect("jni"))(env, 8) != JNI_OK {
                ((**env).ExceptionClear.expect("jni"))(env);
                return Err(HostError::Failed("out of JNI local references".into()));
            }
            let mut result = f(env);
            if ((**env).ExceptionCheck.expect("jni"))(env) != 0 {
                ((**env).ExceptionClear.expect("jni"))(env);
                result = Err("the platform's HttpEngine threw".into());
            }
            ((**env).PopLocalFrame.expect("jni"))(env, std::ptr::null_mut());
            result.map_err(HostError::Failed)
        }
    }

    /// This thread's environment, attaching the thread if it is not.
    fn env(&self) -> Result<*mut JNIEnv, HostError> {
        // SAFETY: `vm` is the process's.
        unsafe {
            let vm = self.vm;
            let mut env: *mut JNIEnv = std::ptr::null_mut();
            let out = (&mut env as *mut *mut JNIEnv).cast::<*mut c_void>();
            if ((**vm).GetEnv.expect("jni"))(vm, out, JNI_VERSION_1_6) != JNI_OK {
                if ((**vm).AttachCurrentThread.expect("jni"))(vm, out, std::ptr::null_mut())
                    != JNI_OK
                {
                    return Err(HostError::Failed("could not attach to the VM".into()));
                }
                ATTACHED.with(|attached| {
                    attached.get_or_init(|| Attached(vm));
                });
            }
            Ok(env)
        }
    }

    unsafe fn text(env: *mut JNIEnv, of: jobject, field: jfieldID) -> String {
        let s = ((**env).GetObjectField.expect("jni"))(env, of, field);
        super::field::text(env, s)
    }

    unsafe fn failure(&self, env: *mut JNIEnv, fetch: jobject) -> String {
        Self::text(env, fetch, self.failure)
    }
}

/// One request's `ImageFetch`, and a byte array its body is read through.
struct Fetch {
    engine: &'static Engine,
    object: jobject,
    chunk: jobject,
}
// SAFETY: global references, valid on every thread; `ImageFetch.cancel` is
// the one call made from another thread, and the engine allows it.
unsafe impl Send for Fetch {}
unsafe impl Sync for Fetch {}
impl Drop for Fetch {
    fn drop(&mut self) {
        let (object, chunk) = (self.object, self.chunk);
        let _ = self.engine.with(|env| unsafe {
            ((**env).CallVoidMethodA.expect("jni"))(env, object, self.engine.cancel, [].as_ptr());
            Ok(())
        });
        // Outside `with`, so they go even when it cannot push a local frame.
        if let Ok(env) = self.engine.env() {
            unsafe {
                ((**env).DeleteGlobalRef.expect("jni"))(env, object);
                ((**env).DeleteGlobalRef.expect("jni"))(env, chunk);
            }
        }
    }
}

/// A body read as it arrives; cancelled when dropped or aborted.
struct Stream {
    fetch: Arc<Fetch>,
    deadline: Option<Instant>,
    _abort: AbortRegistration,
}
impl BodySource for Stream {
    fn read(&mut self, output: &mut [u8]) -> Result<usize, HostError> {
        let Fetch {
            engine,
            object,
            chunk,
        } = *self.fetch;
        engine.with(|env| unsafe {
            let n = ((**env).CallIntMethodA.expect("jni"))(
                env,
                object,
                engine.read,
                [
                    jvalue { l: chunk },
                    jvalue {
                        i: output.len().min(CHUNK as usize) as i32,
                    },
                    jvalue {
                        j: remaining_ms(self.deadline),
                    },
                ]
                .as_ptr(),
            );
            if ((**env).ExceptionCheck.expect("jni"))(env) != 0 {
                return Err("the platform's HttpEngine threw".into());
            }
            if n < 0 {
                return Err(engine.failure(env, object));
            }
            ((**env).GetByteArrayRegion.expect("jni"))(
                env,
                chunk,
                0,
                n,
                output.as_mut_ptr().cast(),
            );
            Ok(n as usize)
        })
    }
}

fn remaining_ms(deadline: Option<Instant>) -> i64 {
    deadline.map_or(i64::MAX, |at| {
        at.saturating_duration_since(Instant::now())
            .as_millis()
            .min(i64::MAX as u128) as i64
    })
}

/// `name\nvalue\n` pairs, the form `ImageFetch` passes headers in.
fn joined(headers: &Headers) -> String {
    headers
        .entries()
        .iter()
        .map(|(name, value)| format!("{name}\n{value}\n"))
        .collect()
}

fn parsed(text: &str) -> Headers {
    let mut headers = Headers::new();
    let mut lines = text.split('\n');
    while let (Some(name), Some(value)) = (lines.next(), lines.next()) {
        headers.append(name, value);
    }
    headers
}

impl Transport for Engine {
    fn open(
        &self,
        request: &Request,
        signal: &AbortSignal,
    ) -> Result<StreamingResponse, HostError> {
        signal.check()?;
        let engine = transport().expect("the process's engine is the one that opens");
        let deadline = request.timeout.map(|timeout| Instant::now() + timeout);
        let fetch = self.with(|env| unsafe {
            let chunk = ((**env).NewByteArray.expect("jni"))(env, CHUNK);
            let url = super::field::java(env, &request.url);
            let headers = super::field::java(env, &joined(&request.headers));
            // Last, so a throw (a header the engine refuses) is the call's
            // final JNI step and `with` clears it.
            let object = ((**env).CallStaticObjectMethodA.expect("jni"))(
                env,
                self.class,
                self.start,
                [jvalue { l: url }, jvalue { l: headers }].as_ptr(),
            );
            if object.is_null() || chunk.is_null() {
                return Err("the platform's HttpEngine did not start".into());
            }
            Ok(Fetch {
                engine,
                object: ((**env).NewGlobalRef.expect("jni"))(env, object),
                chunk: ((**env).NewGlobalRef.expect("jni"))(env, chunk),
            })
        })?;
        let fetch = Arc::new(fetch);
        let cancel = fetch.clone();
        let abort = signal.register(move || {
            let _ = cancel.engine.with(|env| unsafe {
                ((**env).CallVoidMethodA.expect("jni"))(
                    env,
                    cancel.object,
                    cancel.engine.cancel,
                    [].as_ptr(),
                );
                Ok(())
            });
        });
        let (status, headers) = self.with(|env| unsafe {
            let object = fetch.object;
            let got = [jvalue {
                j: remaining_ms(deadline),
            }];
            let came =
                ((**env).CallBooleanMethodA.expect("jni"))(env, object, self.head, got.as_ptr());
            if ((**env).ExceptionCheck.expect("jni"))(env) != 0 {
                return Err("the platform's HttpEngine threw".into());
            }
            if came == 0 {
                return Err(self.failure(env, object));
            }
            let status = ((**env).GetIntField.expect("jni"))(env, object, self.status);
            Ok((
                status as u16,
                parsed(&Self::text(env, object, self.headers)),
            ))
        })?;
        signal.check()?;
        let stream = Stream {
            fetch,
            deadline,
            _abort: abort,
        };
        Ok(StreamingResponse {
            status,
            status_text: String::new(),
            headers,
            body: Body::new(Box::new(stream), request.body_limit(), signal.clone()),
            url: request.url.clone(),
            redirected: false,
        })
    }
}
