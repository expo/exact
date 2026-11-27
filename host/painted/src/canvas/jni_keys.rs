//! The reader's keys and system Back (`canvas/keys.rs`): what `canvas_jni!`
//! defines beside its own, over the same `HOST`.

/// `Native.key`, `Native.namedKey`, `Native.back`, `Native.backProgress`, `Native.open` and `Native.share`, expanded inside
/// `canvas_jni!`'s module, whose `with`, `string` and `flag` they use.
#[macro_export]
#[doc(hidden)]
macro_rules! canvas_jni_keys {
    () => {
        /// A key typed at the focused field: `code` a Unicode scalar, or 0
        /// with `backspace` set; `'\n'` is Enter.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_key(
            _env: *mut JNIEnv,
            _class: jclass,
            code: jint,
            backspace: jboolean,
        ) {
            let ch = char::from_u32(code as u32).filter(|_| code > 0);
            with(|h| h.key(ch, backspace == JNI_TRUE));
        }

        /// A key, down or up, by the web's `code` and `key`: Tab, or a
        /// remote's D-pad arrow or Select (LLP 1115.001.003 D3).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_namedKey(
            env: *mut JNIEnv,
            _class: jclass,
            code: jstring,
            key: jstring,
            down: jboolean,
            repeat: jboolean,
            shift: jboolean,
        ) {
            let (code, key) = (string(env, code), string(env, key));
            let (down, repeat, shift) = (down == JNI_TRUE, repeat == JNI_TRUE, shift == JNI_TRUE);
            with(|h| h.named_key(&code, &key, down, repeat, shift));
        }

        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_hostedBack(
            env: *mut JNIEnv,
            _class: jclass,
            on: jboolean,
        ) -> jboolean {
            // Both the host and its JNI environment belong to this Java thread. Local references
            // live only for one JNI entry, so resolve the reader in the notifying call's frame.
            thread_local! {
                static READER_ENV: std::cell::Cell<*mut JNIEnv> = const { std::cell::Cell::new(std::ptr::null_mut()) };
            }
            fn notify(words: &[i32]) {
                let env = READER_ENV.get();
                unsafe {
                    let class = ((**env).FindClass.expect("jni"))(env, c"dev/exact/canvas/Native".as_ptr());
                    let method = ((**env).GetStaticMethodID.expect("jni"))(env, class, c"requestBack".as_ptr(), c"([I)V".as_ptr());
                    let layers = ints(env, words);
                    ((**env).CallStaticVoidMethodA.expect("jni"))(env, class, method, [$crate::canvas::jni_sys::jvalue { l: layers }].as_ptr());
                    ((**env).DeleteLocalRef.expect("jni"))(env, layers);
                    ((**env).DeleteLocalRef.expect("jni"))(env, class);
                }
            }
            READER_ENV.set(env);
            flag(with(|h| h.hosted_back((on != 0).then_some(notify))).is_some())
        }

        /// System Back: whether the app took it.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_back(
            _env: *mut JNIEnv,
            _class: jclass,
            kind: jint,
            id: jint,
        ) -> jint {
            with(|h| h.back(kind, id as u32)).unwrap_or(0)
        }

        /// Predictive Back as it goes: whether exact2 previews it
        /// (`CanvasHost::back_progress`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_backProgress(
            _env: *mut JNIEnv,
            _class: jclass,
            kind: jint,
            id: jint,
            phase: jint,
            progress: jfloat,
            edge: jint,
        ) -> jboolean {
            flag(with(|h| h.back_progress(kind, id as u32, phase, progress, edge)).unwrap_or(false))
        }

        /// A URL given to the running app (`CanvasHost::open`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_open(
            env: *mut JNIEnv,
            _class: jclass,
            href: jstring,
        ) {
            let href = string(env, href);
            with(|h| h.open(&href));
        }

        /// A share given to the running app (`CanvasHost::share`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_share(
            env: *mut JNIEnv,
            _class: jclass,
            request: jstring,
        ) {
            let request = string(env, request);
            with(|h| h.share(&request));
        }

        /// The first activity's launch URL over a host booted with none
        /// (`CanvasHost::launch`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_launch(
            env: *mut JNIEnv,
            _class: jclass,
            href: jstring,
        ) {
            let href = string(env, href);
            with(|h| h.launch(&href));
        }

        /// That activity's share (`CanvasHost::launch_share`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_launchShare(
            env: *mut JNIEnv,
            _class: jclass,
            request: jstring,
        ) {
            let request = string(env, request);
            with(|h| h.launch_share(&request));
        }
    };
}
