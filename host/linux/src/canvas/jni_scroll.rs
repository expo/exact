//! The scroller's JNI methods, expanded by `canvas_jni!` inside its module,
//! where `with` reaches the thread's host.

#[doc(hidden)]
#[macro_export]
macro_rules! canvas_jni_scroll {
    () => {
        /// The reader moved scroller `id` by (`dx`, `dy`) pixels,
        /// knowing revision `rev` of it.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_scrolled(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            id: $crate::canvas::jni_sys::jint,
            dx: $crate::canvas::jni_sys::jfloat,
            dy: $crate::canvas::jni_sys::jfloat,
            rev: $crate::canvas::jni_sys::jint,
        ) {
            with(|h| h.scrolled(id as u32, dx, dy, rev as u32));
        }

        /// List `id`'s snap areas around a release's projection
        /// (`CanvasHost::snaps_around`; px).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_snapsAround(
            env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            id: $crate::canvas::jni_sys::jint,
            vertical: $crate::canvas::jni_sys::jboolean,
            release: $crate::canvas::jni_sys::jfloat,
            projected: $crate::canvas::jni_sys::jfloat,
        ) -> $crate::canvas::jni_sys::jintArray {
            let words = with(|h| h.snaps_around(id as u32, vertical != 0, release, projected))
                .unwrap_or_else(|| vec![0]);
            ints(env, &words)
        }

        /// The reader's pull to refresh of scroller `id` was let go past its
        /// threshold (`CanvasHost::refresh`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_refresh(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            id: $crate::canvas::jni_sys::jint,
        ) {
            with(|h| h.refresh(id as u32));
        }

        /// A finger began dragging scroller `id`, or (`ended`) let go of it
        /// (`CanvasHost::drag_began`, `drag_ended`; px/s, px), and whether it
        /// throws a fling.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_scrollDrag(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            id: $crate::canvas::jni_sys::jint,
            ended: $crate::canvas::jni_sys::jboolean,
            vx: $crate::canvas::jni_sys::jfloat,
            vy: $crate::canvas::jni_sys::jfloat,
            rest: $crate::canvas::jni_sys::jboolean,
            rest_x: $crate::canvas::jni_sys::jfloat,
            rest_y: $crate::canvas::jni_sys::jfloat,
            flings: $crate::canvas::jni_sys::jboolean,
        ) {
            let rest = (rest != 0).then_some((rest_x, rest_y));
            match ended != 0 {
                false => with(|h| h.drag_began(id as u32)),
                true => with(|h| h.drag_ended(id as u32, vx, vy, rest, flings != 0)),
            };
        }

        /// The fling the reader threw from its drag of scroller `id` stopped
        /// (`CanvasHost::fling_ended`).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_flingEnded(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            id: $crate::canvas::jni_sys::jint,
        ) {
            with(|h| h.fling_ended(id as u32));
        }
    };
}
