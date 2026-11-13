//! `perf frames`' JNI methods (LLP 1079.000 §6.2; `canvas/frames.rs`),
//! expanded by `canvas_jni!` inside its module, where `with` reaches the
//! thread's host. The reader calls each on exact2's thread; times are
//! `System.nanoTime`, `CLOCK_MONOTONIC`.

#[doc(hidden)]
#[macro_export]
macro_rules! canvas_jni_frames {
    () => {
        /// A Choreographer turn (`Owner.turned`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_frameTurn(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            arrival: i64,
            vsync: i64,
            deadline: i64,
            period: i64,
            finished: i64,
            publication: i64,
        ) {
            let [arrival, vsync, deadline, period, finished, publication] =
                [arrival, vsync, deadline, period, finished, publication].map(|v| v.max(0) as u64);
            with(|h| h.frame_turn(arrival, vsync, deadline, period, finished, publication));
        }

        /// The main thread ingested every publication through `applied`.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_frameLanded(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            applied: i64,
            began: i64,
            ended: i64,
        ) {
            let [applied, began, ended] = [applied, began, ended].map(|v| v.max(0) as u64);
            with(|h| h.frame_landed(applied, began, ended));
        }

        /// No turn is asked for.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_frameIdle(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
        ) {
            with(|h| h.frame_idle());
        }

        /// The display's refresh period, nanoseconds.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_framePeriod(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            period: i64,
        ) {
            let period = period.max(0) as u64;
            if with(|h| h.frame_period(period)).is_none() {
                $crate::canvas::frames::display_period(period);
            }
        }

        /// `count` window frames of five words, and the reports lost.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_frameWindow(
            env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            words: $crate::canvas::jni_sys::jlongArray,
            count: i32,
            lost: i64,
        ) {
            let length = ((**env).GetArrayLength.expect("jni"))(env, words);
            let n = (count.max(0) * 5).min(length);
            let mut v = vec![0i64; n as usize];
            ((**env).GetLongArrayRegion.expect("jni"))(env, words, 0, n, v.as_mut_ptr());
            with(|h| h.window_frames(&v, lost.max(0) as u64));
        }

        /// The device a report names, as JSON in UTF-8.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_frameIdentity(
            env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            identity: $crate::canvas::jni_sys::jbyteArray,
        ) {
            $crate::canvas::frames::identity(&$crate::canvas::jni::utf8(env, identity));
        }

        /// The frame rates GPU canvases ask for that changed: view and rate
        /// pairs, or null.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_frameRates(
            env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
        ) -> $crate::canvas::jni_sys::jintArray {
            let Some(rates) = with(|h| h.frame_rates()).flatten() else {
                return std::ptr::null_mut();
            };
            let arr = ((**env).NewIntArray.expect("jni"))(env, rates.len() as i32);
            ((**env).SetIntArrayRegion.expect("jni"))(
                env,
                arr,
                0,
                rates.len() as i32,
                rates.as_ptr(),
            );
            arr
        }
    };
}
