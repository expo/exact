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

        /// After the reader's scroll step: whether its frame would only
        /// move what the reader moved itself (`CanvasHost::moved`). The
        /// move is then taken, and there is no stream.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_moved(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
        ) -> $crate::canvas::jni_sys::jboolean {
            if PRIMED.with(|p| p.borrow().is_some()) {
                return $crate::canvas::jni_sys::JNI_FALSE;
            }
            let moved = with(|h| h.moved()).unwrap_or(false);
            if moved {
                // As after a move's stream: the next paint is drawn,
                // however like the last.
                DRAWN.with(|d| *d.borrow_mut() = None);
                $crate::presenter::preferences::scrub_frame_drawn();
            }
            flag(moved)
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

        /// The sheet shared by a native list drag moved or settled.
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_sheetDrag(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            index: $crate::canvas::jni_sys::jint,
            generation: $crate::canvas::jni_sys::jint,
            top: $crate::canvas::jni_sys::jfloat,
            ended: $crate::canvas::jni_sys::jboolean,
            velocity: $crate::canvas::jni_sys::jfloat,
        ) {
            with(|h| {
                h.sheet_drag(
                    index as u32,
                    generation as u32,
                    top,
                    (ended != 0).then_some(velocity),
                )
            });
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

        /// The reader's pinch of zoomable scroller `id`
        /// (`CanvasHost::zoom`; px).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_zoom(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            id: $crate::canvas::jni_sys::jint,
            phase: $crate::canvas::jni_sys::jint,
            factor: $crate::canvas::jni_sys::jfloat,
            x0: $crate::canvas::jni_sys::jfloat,
            y0: $crate::canvas::jni_sys::jfloat,
            x1: $crate::canvas::jni_sys::jfloat,
            y1: $crate::canvas::jni_sys::jfloat,
        ) {
            with(|h| h.zoom(id as u32, phase, factor, (x0, y0), (x1, y1)));
        }

        /// The reader's double tap (`CanvasHost::zoom_tap`; px).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_zoomTap(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            phase: $crate::canvas::jni_sys::jint,
            x: $crate::canvas::jni_sys::jfloat,
            y: $crate::canvas::jni_sys::jfloat,
        ) {
            with(|h| h.zoom_tap(phase, x, y));
        }

        /// A wheel with `ctrl` held (`CanvasHost::zoom_wheel`; px, points).
        #[no_mangle]
        pub unsafe extern "system" fn Java_dev_exact_canvas_Native_zoomWheel(
            _env: *mut $crate::canvas::jni_sys::JNIEnv,
            _class: $crate::canvas::jni_sys::jclass,
            x: $crate::canvas::jni_sys::jfloat,
            y: $crate::canvas::jni_sys::jfloat,
            dy: $crate::canvas::jni_sys::jfloat,
        ) {
            with(|h| h.zoom_wheel(x, y, dy));
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
