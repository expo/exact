use super::{jvm::bind, *};
use crate::surfaces::{
    digest,
    tests::{compile_fixture, fixture},
};
use serde_json::Value;
use std::path::PathBuf;

/// A stand-in for the Java VM on the test's thread: JNI's two function
/// tables with the functions the host calls, each of which logs its name.
mod vm {
    use jni_sys::{
        jboolean, jint, jobject, JNIEnv, JNIInvokeInterface_, JNINativeInterface_, JavaVM,
        JNI_EDETACHED, JNI_OK,
    };
    use std::{
        cell::{Cell, RefCell},
        ffi::c_void,
    };

    thread_local! {
        static CALLS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
        static HANDLES: Cell<(*mut JNIEnv, *mut JavaVM)> =
            const { Cell::new((std::ptr::null_mut(), std::ptr::null_mut())) };
        /// A Java exception is pending on the thread.
        pub static PENDING: Cell<bool> = const { Cell::new(false) };
        /// The thread is attached to the VM.
        pub static ATTACHED: Cell<bool> = const { Cell::new(false) };
        /// `NewGlobalRef` has no memory: it returns null and throws.
        pub static EXHAUSTED: Cell<bool> = const { Cell::new(false) };
    }

    /// The thread's environment and its VM.
    pub fn start() -> (*mut JNIEnv, *mut JavaVM) {
        // SAFETY: a table of null pointers and absent functions.
        let (env, vm) = unsafe {
            (
                JNINativeInterface_ {
                    GetJavaVM: Some(java_vm),
                    NewGlobalRef: Some(global),
                    ExceptionCheck: Some(check),
                    ExceptionDescribe: Some(describe),
                    ExceptionClear: Some(clear),
                    ..std::mem::zeroed()
                },
                JNIInvokeInterface_ {
                    GetEnv: Some(env),
                    AttachCurrentThread: Some(attach),
                    DetachCurrentThread: Some(detach),
                    ..std::mem::zeroed()
                },
            )
        };
        let handles = (
            Box::into_raw(Box::new(Box::into_raw(Box::new(env)).cast_const())),
            Box::into_raw(Box::new(Box::into_raw(Box::new(vm)).cast_const())),
        );
        HANDLES.set(handles);
        handles
    }

    /// The calls since the last reading, in order.
    pub fn calls() -> Vec<&'static str> {
        CALLS.take()
    }

    /// The global reference `NewGlobalRef` gives for `local`.
    pub fn kept(local: jobject) -> jobject {
        local.wrapping_byte_add(1)
    }

    fn log(name: &'static str) {
        CALLS.with_borrow_mut(|calls| calls.push(name));
    }

    unsafe extern "system" fn java_vm(_: *mut JNIEnv, out: *mut *mut JavaVM) -> jint {
        log("GetJavaVM");
        unsafe { *out = HANDLES.get().1 };
        JNI_OK
    }
    unsafe extern "system" fn global(_: *mut JNIEnv, local: jobject) -> jobject {
        log("NewGlobalRef");
        if EXHAUSTED.get() {
            PENDING.set(true);
            return std::ptr::null_mut();
        }
        kept(local)
    }
    unsafe extern "system" fn check(_: *mut JNIEnv) -> jboolean {
        log("ExceptionCheck");
        PENDING.get().into()
    }
    unsafe extern "system" fn describe(_: *mut JNIEnv) {
        log("ExceptionDescribe");
    }
    unsafe extern "system" fn clear(_: *mut JNIEnv) {
        log("ExceptionClear");
        PENDING.set(false);
    }
    unsafe extern "system" fn env(_: *mut JavaVM, out: *mut *mut c_void, _: jint) -> jint {
        log("GetEnv");
        if !ATTACHED.get() {
            return JNI_EDETACHED;
        }
        unsafe { *out = HANDLES.get().0.cast() };
        JNI_OK
    }
    unsafe extern "system" fn attach(
        _: *mut JavaVM,
        out: *mut *mut c_void,
        _: *mut c_void,
    ) -> jint {
        log("AttachCurrentThread");
        ATTACHED.set(true);
        unsafe { *out = HANDLES.get().0.cast() };
        JNI_OK
    }
    unsafe extern "system" fn detach(_: *mut JavaVM) -> jint {
        log("DetachCurrentThread");
        ATTACHED.set(false);
        JNI_OK
    }
}

/// The probe module with the entry, as an image of its own beside `probe`:
/// it counts its calls and the exports called before it, and fails when
/// `fails`.
fn initializing(probe: &Path, compat: &Value, name: &str, fails: bool) -> (PathBuf, Value) {
    let entry = format!(
        r#"
static void *given[3];
uint32_t gpu_android_init(void *vm, void *env, void *context) {{
  inits++; given[0] = vm; given[1] = env; given[2] = context; failed = {fails}; return failed;
}}
uint32_t test_inits(void) {{ return inits; }}
uint32_t test_early(void) {{ return early; }}
uint32_t test_loads(void) {{ return loads; }}
uintptr_t test_given(uint32_t at) {{ return (uintptr_t)given[at]; }}
"#,
        fails = u32::from(fails)
    );
    let source = std::fs::read_to_string(probe.with_file_name("probe.c")).unwrap();
    let mut edited = source.clone();
    for (from, to) in [
        (
            "void gpu_load_headless(void) {}",
            "static uint32_t inits, early, loads, failed;\n\
             void gpu_load_headless(void) { early += !inits; loads++; }",
        ),
        (
            "uint32_t gpu_tier(uint32_t code) {",
            "uint32_t gpu_tier(uint32_t code) { early += !inits;",
        ),
        (
            "uint32_t gpu_error(void) { return 0; }",
            "uint32_t gpu_error(void) {\n\
             if (!failed) return 0; failed = 0; output = \"no AssetManager\"; return 15; }",
        ),
    ] {
        assert!(edited.contains(from), "the probe has `{from}`");
        edited = edited.replace(from, to);
    }
    let path = probe.with_file_name(format!("lib{name}.{}", std::env::consts::DLL_EXTENSION));
    let source = path.with_extension("c");
    let mut logged = String::from(
        "#include <string.h>\n\
         static char calls[4096];\n\
         static void test_log(const char *name) { strcat(calls, name); strcat(calls, \",\"); }\n\
         const char *test_calls(void) { return calls; }\n",
    );
    // Record every GPU export, including teardown and the failure's error reads.
    for line in (edited + &entry).lines() {
        let name = line
            .split_once('(')
            .and_then(|(head, _)| head.split_whitespace().last())
            .filter(|name| name.starts_with("gpu_"));
        if let Some(name) = name {
            logged.push_str(&line.replacen('{', &format!("{{ test_log(\"{name}\");"), 1));
        } else {
            logged.push_str(line);
        }
        logged.push('\n');
    }
    std::fs::write(&source, logged).unwrap();
    compile_fixture(&source, &path);
    let mut compat = compat.clone();
    compat["embedded"]["gpu"]["name"] = path.file_name().unwrap().to_str().unwrap().into();
    compat["embedded"]["gpu"]["sha256"] = digest::file(&path, true).unwrap().into();
    (path, compat)
}

fn read(library: &Library, name: &[u8]) -> u32 {
    // SAFETY: the fixture's observers take nothing and return a count.
    unsafe { library.get::<unsafe extern "C" fn() -> u32>(name).unwrap()() }
}

fn calls(library: &Library) -> String {
    // SAFETY: the fixture owns a nul-terminated log, read while it is idle.
    unsafe {
        let read = library
            .get::<unsafe extern "C" fn() -> *const std::ffi::c_char>(b"test_calls")
            .unwrap();
        std::ffi::CStr::from_ptr(read()).to_str().unwrap().into()
    }
}

#[test]
fn the_entry_runs_once_for_an_image_before_any_other_export() {
    let (probe, plain) = fixture();
    let (env, vm) = vm::start();
    let local = std::ptr::without_provenance_mut(64);
    // An artifact without the entry needs no application.
    drop(Abi::open_path(&probe, &plain, "").unwrap());

    // No memory for the global reference: the exception stays pending for
    // the reader's `trust` to throw, the check is the last JNI call, and no
    // application is bound.
    vm::EXHAUSTED.set(true);
    // SAFETY: the stand-in's environment, on its thread.
    assert!(!unsafe { bind(env, local) });
    assert_eq!(vm::calls(), ["GetJavaVM", "NewGlobalRef", "ExceptionCheck"]);
    assert!(vm::PENDING.replace(false));
    let (path, compat) = initializing(&probe, &plain, "ok", false);
    // Keep the image's log even if the refused open releases its handle.
    // SAFETY: the fixture, to observe its calls.
    let observer = unsafe { Library::new(&path) }.unwrap();
    let unbound = Abi::open_path(&path, &compat, "").err().unwrap();
    assert!(unbound.ends_with(UNBOUND), "{unbound}");
    let mut refused_calls = vec![("unbound", calls(&observer))];
    // The application is bound once.
    vm::EXHAUSTED.set(false);
    // SAFETY: as above.
    assert!(unsafe { bind(env, local) && bind(env, local) });
    assert_eq!(vm::calls(), ["GetJavaVM", "NewGlobalRef"]);

    // The refused open did not call the entry, so this one does, on a thread
    // the host attaches for the call: before the tier and the load an open
    // makes, with the VM, the thread's environment and the global reference.
    let first = Abi::open_path(&path, &compat, "").unwrap();
    assert_eq!(
        vm::calls(),
        [
            "GetEnv",
            "AttachCurrentThread",
            "ExceptionCheck",
            "DetachCurrentThread"
        ]
    );
    let counts = |abi: &Abi| {
        [b"test_inits".as_slice(), b"test_early", b"test_loads"].map(|n| read(&abi.library, n))
    };
    assert_eq!(counts(&first), [1, 0, 1]);
    // SAFETY: the fixture's observer of the entry's arguments.
    let given = unsafe { first.symbol::<unsafe extern "C" fn(u32) -> usize>(b"test_given") };
    assert_eq!(
        [0, 1, 2].map(|at| unsafe { given(at) }),
        [vm as usize, env as usize, vm::kept(local) as usize]
    );
    // The host's own open beside the preparation worker's, and one after
    // every handle closed: the image stayed loaded and is not asked again.
    let second = Abi::open_path(&path, &compat, "").unwrap();
    assert_eq!(counts(&second), [1, 0, 2]);
    drop((first, second));
    assert!(calls(&observer).ends_with("gpu_unload,gpu_unload,"));
    drop(observer);
    assert_eq!(
        counts(&Abi::open_path(&path, &compat, "").unwrap()),
        [1, 0, 3]
    );
    assert!(vm::calls().is_empty());

    // A failure refuses this open and every later one, by name, and nothing
    // else of the artifact runs. A thread Java attached stays attached.
    vm::ATTACHED.set(true);
    let (path, compat) = initializing(&probe, &plain, "fails", true);
    // SAFETY: the fixture, to observe both refused opens.
    let failed = unsafe { Library::new(&path) }.unwrap();
    for case in ["failed", "repeated failure"] {
        assert_eq!(
            Abi::open_path(&path, &compat, "").err().unwrap(),
            format!(
                "GPU module {}: Android initialization failed: no AssetManager",
                path.display()
            )
        );
        refused_calls.push((case, calls(&failed)));
    }
    assert_eq!(vm::calls(), ["GetEnv", "ExceptionCheck"]);
    assert_eq!(
        [b"test_inits".as_slice(), b"test_loads"].map(|n| read(&failed, n)),
        [1, 0]
    );

    // An exception the entry leaves pending (set here in its place) refuses
    // the artifact and is cleared before the host returns to Java.
    let (path, compat) = initializing(&probe, &plain, "throws", false);
    // SAFETY: the fixture, to observe the exception refusal.
    let throwing = unsafe { Library::new(&path) }.unwrap();
    vm::PENDING.set(true);
    let threw = Abi::open_path(&path, &compat, "").err().unwrap();
    assert!(
        threw.ends_with("it left a Java exception pending"),
        "{threw}"
    );
    assert_eq!(
        vm::calls(),
        [
            "GetEnv",
            "ExceptionCheck",
            "ExceptionDescribe",
            "ExceptionClear"
        ]
    );
    refused_calls.push(("exception", calls(&throwing)));
    assert_eq!(
        refused_calls,
        [
            ("unbound", String::new()),
            ("failed", "gpu_android_init,gpu_error,gpu_out_ptr,".into()),
            (
                "repeated failure",
                "gpu_android_init,gpu_error,gpu_out_ptr,".into()
            ),
            ("exception", "gpu_android_init,".into()),
        ]
    );
}
