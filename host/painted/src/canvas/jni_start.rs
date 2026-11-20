//! The reader's `start`, `startAsync` and `adopt`, expanded by `canvas_jni!`
//! inside its module, where `HOST`, `hatches` and `updates` are the app's.

#[doc(hidden)]
#[macro_export]
macro_rules! canvas_jni_start {
    () => {
        /// The running generation's asset directory, or null for the APK's
        /// own (`delivery::reader_root`, LLP 1030.006 D1).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_assetRoot(
            env: *mut JNIEnv,
            _class: jclass,
        ) -> jstring {
            match $crate::delivery::reader_root() {
                Some(dir) => $crate::canvas::field::java(env, &dir.to_string_lossy()),
                None => std::ptr::null_mut(),
            }
        }

        /// The process environment the host reads, from the reader's files
        /// directory `dir`.
        unsafe fn environment(env: *mut JNIEnv, dir: jstring, live: jboolean) {
            let dir = string(env, dir);
            pipeline_cache();
            std::env::set_var("HOME", &dir);
            std::env::set_var("XDG_DATA_HOME", format!("{dir}/data"));
            std::env::set_var("EXACT_ASSETS", format!("{dir}/app"));
            $crate::app::show_material_look();
            std::env::set_var("EXACT_FONTS", "/system/fonts");
            std::env::set_var("EXACT_FONT", "Roboto");
            std::env::set_var("BENCH_LIVE", if live == JNI_TRUE { "1" } else { "0" });
        }

        /// Boot the app, with its update store under `store` (the reader's
        /// no-backup directory) when the library links one.
        fn boot(size: (u32, u32), scale: f32, store: &str) -> Result<Host, String> {
            static LAUNCH: std::sync::Once = std::sync::Once::new();
            LAUNCH.call_once(launch_parts);
            let updates = updates().map(|open| (open, std::path::Path::new(store)));
            Host::boot(
                exact_plan(),
                exact_compat(),
                size,
                scale,
                hatches(),
                updates,
            )
        }

        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_start(
            env: *mut JNIEnv,
            _class: jclass,
            width: jint,
            height: jint,
            scale: jfloat,
            dir: jstring,
            store: jstring,
            live: jboolean,
        ) -> jboolean {
            static LOG: std::sync::Once = std::sync::Once::new();
            LOG.call_once($crate::android::redirect_stdio);
            // The activity was made again in this process: its reader
            // adopts the host that runs. Nothing boots, and the
            // environment, which other threads read by now, is left alone.
            if adopt_running((width as u32, height as u32), scale) {
                return JNI_TRUE;
            }
            // The reader hosts the app's text fields (LLP 1115.001 D8).
            $crate::canvas::field::attach(env, scale);
            $crate::canvas::legacy::attach(env);
            $crate::canvas::presence::attach(env);
            $crate::canvas::secrets::attach(env);
            environment(env, dir, live);
            let store = string(env, store);
            match boot((width as u32, height as u32), scale, &store) {
                Ok(h) => {
                    HOST.with(|slot| *slot.borrow_mut() = Some(h));
                    RUNNING.store(true, std::sync::atomic::Ordering::Release);
                    $crate::android::release_library_pages_after(RELEASE_AFTER);
                    JNI_TRUE
                }
                Err(e) => {
                    $crate::android::log(&format!("exact: boot: {e}"));
                    JNI_FALSE
                }
            }
        }

        /// What a booting thread hands the thread that runs the host: the
        /// host and its first frame. The
        /// host's `Rc`s move with it whole; nothing on the booting thread
        /// keeps one (its image and executor threads hold only `Send` state).
        struct Booted(Host, Option<Vec<u32>>);
        // SAFETY: see above: the booting thread lets go of all of it.
        unsafe impl Send for Booted {}
        static BOOTED: std::sync::Mutex<Option<Result<Booted, String>>> =
            std::sync::Mutex::new(None);
        static READY: std::sync::Condvar = std::sync::Condvar::new();
        thread_local! {
            /// The first frame a booting thread painted, returned by the next `frame`.
            static PRIMED: RefCell<Option<Vec<u32>>> = const { RefCell::new(None) };
        }

        /// `start`, on a thread of its own: the host boots and paints its first
        /// frame while the reader sets up; `adopt` takes it.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_startAsync(
            env: *mut JNIEnv,
            _class: jclass,
            width: jint,
            height: jint,
            scale: jfloat,
            dir: jstring,
            store: jstring,
            live: jboolean,
        ) {
            static LOG: std::sync::Once = std::sync::Once::new();
            LOG.call_once($crate::android::redirect_stdio);
            // A host runs already (the activity was made again): nothing
            // boots, and `adopt` adopts that one at this size.
            if RUNNING.load(std::sync::atomic::Ordering::Acquire) {
                *ADOPT.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((width as u32, height as u32, scale));
                return;
            }
            $crate::canvas::field::attach(env, scale);
            $crate::canvas::legacy::attach(env);
            $crate::canvas::presence::attach(env);
            $crate::canvas::secrets::attach(env);
            // The environment is set here, before any other thread reads it.
            environment(env, dir, live);
            let store = string(env, store);
            std::thread::Builder::new()
                .name("exact-boot".into())
                .spawn(move || {
                    let booted = $crate::android::trace(c"exact boot", || {
                        boot((width as u32, height as u32), scale, &store).map(|mut h| {
                            h.set_borrowed(true);
                            let first = $crate::canvas::agent::after_turn(h.frame(false));
                            h.roles_here(false);
                            Booted(h, first)
                        })
                    });
                    *BOOTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(booted);
                    READY.notify_all();
                })
                .expect("boot thread");
        }

        /// Take the host `startAsync` booted, waiting for it; whether it booted.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_adopt(
            _env: *mut JNIEnv,
            _class: jclass,
        ) -> jboolean {
            let again = ADOPT.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Some((width, height, scale)) = again {
                return flag(adopt_running((width, height), scale));
            }
            let mut slot = BOOTED.lock().unwrap_or_else(|e| e.into_inner());
            while slot.is_none() {
                slot = READY.wait(slot).unwrap_or_else(|e| e.into_inner());
            }
            match slot.take().expect("booted") {
                Ok(Booted(mut h, first)) => {
                    h.set_borrowed(false);
                    h.roles_here(true);
                    HOST.with(|s| *s.borrow_mut() = Some(h));
                    RUNNING.store(true, std::sync::atomic::Ordering::Release);
                    PRIMED.with(|p| *p.borrow_mut() = first);
                    $crate::android::release_library_pages_after(RELEASE_AFTER);
                    JNI_TRUE
                }
                Err(e) => {
                    $crate::android::log(&format!("exact: boot: {e}"));
                    JNI_FALSE
                }
            }
        }
    };
}
